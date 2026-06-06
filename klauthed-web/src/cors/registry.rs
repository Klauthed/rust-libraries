//! The [`CorsOriginRegistry`] trait and its in-memory + caching implementations
//! for dynamic, per-request origin checks.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use async_trait::async_trait;

/// Decides whether a given `Origin` header value is allowed to make
/// cross-origin requests.
///
/// Implement this trait against your data layer. A typical IDP implementation
/// queries a `tenant_allowed_origins` table (or checks a per-tenant config
/// field) and returns `true` when the origin matches a record for an active
/// tenant.
///
/// **Performance:** wrap your implementation in [`CachedOriginRegistry`] so
/// each origin is looked up at most once per TTL window.
///
/// # Example skeleton
///
/// ```ignore
/// struct TenantOriginRegistry { pool: sqlx::AnyPool }
///
/// #[async_trait::async_trait]
/// impl CorsOriginRegistry for TenantOriginRegistry {
///     async fn is_allowed(&self, origin: &str) -> bool {
///         sqlx::query_scalar!(
///             "SELECT COUNT(*) FROM tenant_origins WHERE origin = $1 AND active = 1",
///             origin
///         )
///         .fetch_one(&self.pool)
///         .await
///         .ok()
///         .map(|n: i64| n > 0)
///         .unwrap_or(false)
///     }
/// }
/// ```
#[async_trait]
pub trait CorsOriginRegistry: Send + Sync + 'static {
    /// Return `true` if `origin` (exact `Origin:` header value, e.g.
    /// `"https://app.acme.com"`) may make cross-origin requests.
    async fn is_allowed(&self, origin: &str) -> bool;
}

// ── InMemoryOriginRegistry ────────────────────────────────────────────────────

/// An in-memory [`CorsOriginRegistry`] backed by a `HashSet<String>`.
///
/// Suitable for static origins known at startup and for tests. The set is
/// shared behind an `Arc<RwLock>` so it can be updated at runtime (e.g. from
/// an admin endpoint or a background sync task) without restarting the server.
///
/// ```
/// use klauthed_web::cors::InMemoryOriginRegistry;
///
/// let reg = InMemoryOriginRegistry::with_origins([
///     "https://auth.klauthed.com",
///     "https://admin.klauthed.com",
/// ]);
/// ```
#[derive(Clone, Default)]
pub struct InMemoryOriginRegistry {
    origins: Arc<RwLock<HashSet<String>>>,
}

impl InMemoryOriginRegistry {
    /// An empty registry (no origins allowed).
    pub fn new() -> Self {
        Self::default()
    }

    /// Pre-populate with `origins` (builder form).
    pub fn with_origins(origins: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let reg = Self::new();
        for o in origins {
            reg.insert(o.into());
        }
        reg
    }

    /// Add one origin (builder form, for chaining).
    pub fn with_origin(self, origin: impl Into<String>) -> Self {
        self.insert(origin.into());
        self
    }

    /// Add an origin at runtime (e.g. from an admin endpoint).
    pub fn insert(&self, origin: impl Into<String>) {
        self.origins
            .write()
            .expect("InMemoryOriginRegistry lock poisoned")
            .insert(origin.into());
    }

    /// Remove an origin at runtime.
    pub fn remove(&self, origin: &str) -> bool {
        self.origins
            .write()
            .expect("InMemoryOriginRegistry lock poisoned")
            .remove(origin)
    }

    /// How many origins are currently allowed.
    pub fn len(&self) -> usize {
        self.origins
            .read()
            .expect("InMemoryOriginRegistry lock poisoned")
            .len()
    }

    /// Whether no origins are allowed.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[async_trait]
impl CorsOriginRegistry for InMemoryOriginRegistry {
    async fn is_allowed(&self, origin: &str) -> bool {
        self.origins
            .read()
            .expect("InMemoryOriginRegistry lock poisoned")
            .contains(origin)
    }
}

// ── CachedOriginRegistry ──────────────────────────────────────────────────────

/// A [`CorsOriginRegistry`] that caches results from an inner registry for
/// `ttl` so expensive lookups (DB, HTTP) are not repeated on every request.
///
/// On a **cache hit** the cached `allowed`/`denied` decision is returned
/// immediately. On a **miss** (new origin or expired entry) the inner registry
/// is queried and the result is stored. This means a newly registered domain
/// becomes effective within at most one TTL window.
///
/// ```
/// use std::{sync::Arc, time::Duration};
/// use klauthed_web::cors::{CachedOriginRegistry, InMemoryOriginRegistry};
///
/// let inner = InMemoryOriginRegistry::with_origins(["https://app.example.com"]);
/// let cached = CachedOriginRegistry::new(inner, Duration::from_secs(300));
/// let shared: Arc<CachedOriginRegistry<_>> = Arc::new(cached);
/// ```
pub struct CachedOriginRegistry<R> {
    inner: R,
    /// (allowed, cached_at)
    cache: Mutex<HashMap<String, (bool, Instant)>>,
    ttl: Duration,
}

impl<R: CorsOriginRegistry> CachedOriginRegistry<R> {
    /// Wrap `inner` with a cache whose entries live for `ttl`.
    pub fn new(inner: R, ttl: Duration) -> Self {
        Self {
            inner,
            cache: Mutex::new(HashMap::new()),
            ttl,
        }
    }

    /// Clear the entire cache (useful after bulk origin updates).
    pub fn invalidate_all(&self) {
        self.cache
            .lock()
            .expect("CachedOriginRegistry lock poisoned")
            .clear();
    }

    /// Remove a single origin from the cache so the next request re-checks.
    pub fn invalidate(&self, origin: &str) {
        self.cache
            .lock()
            .expect("CachedOriginRegistry lock poisoned")
            .remove(origin);
    }
}

#[async_trait]
impl<R: CorsOriginRegistry> CorsOriginRegistry for CachedOriginRegistry<R> {
    async fn is_allowed(&self, origin: &str) -> bool {
        // Fast path: cache hit inside a scoped lock (released before the await).
        {
            let cache = self.cache.lock().expect("cache lock poisoned");
            if let Some(&(allowed, cached_at)) = cache.get(origin)
                && cached_at.elapsed() < self.ttl
            {
                return allowed;
            }
        }

        // Slow path: query inner registry and update cache.
        let allowed = self.inner.is_allowed(origin).await;
        self.cache
            .lock()
            .expect("cache lock poisoned")
            .insert(origin.to_owned(), (allowed, Instant::now()));
        allowed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[actix_web::test]
    async fn in_memory_registry_allows_inserted_origins() {
        let reg = InMemoryOriginRegistry::with_origins(["https://app.acme.com"]);
        assert!(reg.is_allowed("https://app.acme.com").await);
        assert!(!reg.is_allowed("https://attacker.example.com").await);
    }

    #[actix_web::test]
    async fn in_memory_registry_runtime_insert_and_remove() {
        let reg = InMemoryOriginRegistry::new();
        assert!(!reg.is_allowed("https://new.example.com").await);

        reg.insert("https://new.example.com");
        assert!(reg.is_allowed("https://new.example.com").await);

        reg.remove("https://new.example.com");
        assert!(!reg.is_allowed("https://new.example.com").await);
    }

    #[actix_web::test]
    async fn cached_registry_returns_same_result_within_ttl() {
        let inner = InMemoryOriginRegistry::with_origins(["https://a.example.com"]);
        let cached = CachedOriginRegistry::new(inner.clone(), Duration::from_secs(60));

        assert!(cached.is_allowed("https://a.example.com").await);
        // Add a second origin to the inner registry.
        inner.insert("https://b.example.com");
        // This one is not cached yet → goes to inner → allowed.
        assert!(cached.is_allowed("https://b.example.com").await);
    }

    #[actix_web::test]
    async fn cached_registry_invalidate_clears_entry() {
        let inner = InMemoryOriginRegistry::with_origins(["https://a.example.com"]);
        let cached = CachedOriginRegistry::new(inner.clone(), Duration::from_secs(3600));

        assert!(cached.is_allowed("https://a.example.com").await);
        // Remove from inner and invalidate cache.
        inner.remove("https://a.example.com");
        cached.invalidate("https://a.example.com");
        // Now re-queries inner which says not allowed.
        assert!(!cached.is_allowed("https://a.example.com").await);
    }
}
