//! CORS configuration — static and dynamic.
//!
//! Two complementary approaches, both ready to mount with `.wrap(...)`:
//!
//! ## Static CORS — [`build_cors`]
//!
//! Origins are fixed at startup. Good for services with a known, small set of
//! allowed frontends.
//!
//! ```no_run
//! use actix_web::App;
//! use klauthed_web::cors::{CorsConfig, build_cors};
//!
//! let _app = App::new().wrap(build_cors(&CorsConfig::permissive()));
//! ```
//!
//! ## Dynamic CORS — [`DynamicCors`]
//!
//! Origins are checked at request time via a pluggable [`CorsOriginRegistry`].
//! This is the right approach for a multi-tenant IDP where customers register
//! their own auth-page domains:
//!
//! * Your platform's own frontends live in [`CorsConfig::allowed_origins`]
//!   (checked in O(1) without any I/O).
//! * Every tenant's registered domains are resolved by the registry.
//! * Wrap the registry in [`CachedOriginRegistry`] so each origin is looked
//!   up at most once per TTL window, not on every request.
//!
//! ```no_run
//! use std::sync::Arc;
//! use std::time::Duration;
//! use actix_web::App;
//! use klauthed_web::cors::{
//!     CorsConfig, CachedOriginRegistry, DynamicCors, InMemoryOriginRegistry,
//! };
//!
//! // Your own auth frontend — always allowed, no I/O needed.
//! let config = CorsConfig {
//!     allowed_origins: vec!["https://auth.klauthed.com".into()],
//!     allow_credentials: true,
//!     ..CorsConfig::default()
//! };
//!
//! // Tenant domains fetched from your DB, cached for 5 minutes.
//! // In production, replace InMemoryOriginRegistry with your own
//! // TenantOriginRegistry that queries the tenant table.
//! let registry = Arc::new(CachedOriginRegistry::new(
//!     InMemoryOriginRegistry::new(),
//!     Duration::from_secs(300),
//! ));
//!
//! let cors = DynamicCors::new(config, registry);
//!
//! // Mount CORS *outside* JwtAuth so OPTIONS preflight is answered before
//! // auth checks run.
//! let _app = App::new()
//!     .wrap(klauthed_web::auth::JwtAuth::new())
//!     .wrap(cors);
//! ```
//!
//! ## Security notes
//!
//! * CORS must be mounted **outer** (last in `.wrap()` chain) so it runs
//!   first on incoming requests and handles `OPTIONS` before auth.
//! * Always send `Vary: Origin` when echoing a specific origin — this tells
//!   CDNs and proxies that the response differs per caller. Both middlewares
//!   here do this automatically.
//! * Never combine `Access-Control-Allow-Origin: *` with
//!   `Access-Control-Allow-Credentials: true` — browsers reject it.
//! * Keep allowed headers minimal: exposed headers become readable by
//!   cross-origin JS.

use std::collections::{HashMap, HashSet};
use std::future::{ready, Ready};
use std::rc::Rc;
use std::sync::{Arc, Mutex, RwLock};
use std::task::{Context as TaskContext, Poll};
use std::time::{Duration, Instant};

use actix_cors::Cors;
use actix_web::body::BoxBody;
use actix_web::dev::{Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::http::header::{
    HeaderMap, HeaderValue, ACCESS_CONTROL_ALLOW_CREDENTIALS, ACCESS_CONTROL_ALLOW_HEADERS,
    ACCESS_CONTROL_ALLOW_METHODS, ACCESS_CONTROL_ALLOW_ORIGIN, ACCESS_CONTROL_EXPOSE_HEADERS,
    ACCESS_CONTROL_MAX_AGE, ORIGIN, VARY,
};
use actix_web::http::Method;
use actix_web::{Error, HttpResponse};
use async_trait::async_trait;
use futures_util::future::LocalBoxFuture;
use serde::{Deserialize, Serialize};

// ── Static CORS ───────────────────────────────────────────────────────────────

/// Configuration for Cross-Origin Resource Sharing headers.
///
/// Used by both [`build_cors`] (static middleware) and [`DynamicCors`] (as
/// the policy + platform-level static origins). Implements
/// [`serde::Deserialize`] so it can live in a service config file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CorsConfig {
    /// Allowed origin patterns. An entry of `"*"` means allow all origins
    /// (development only — incompatible with `allow_credentials = true`).
    /// An empty list produces no CORS headers (same-origin only).
    ///
    /// In [`DynamicCors`] these are the *platform* origins that are always
    /// allowed without any registry lookup.
    pub allowed_origins: Vec<String>,

    /// HTTP methods allowed in cross-origin requests.
    pub allowed_methods: Vec<String>,

    /// Request headers allowed in cross-origin requests.
    pub allowed_headers: Vec<String>,

    /// Response headers exposed to browser JavaScript.
    pub expose_headers: Vec<String>,

    /// Whether to allow cookies and `Authorization` credentials.
    /// Must be `false` when `allowed_origins` contains `"*"`.
    pub allow_credentials: bool,

    /// Preflight cache lifetime in seconds (`None` omits the header).
    pub max_age_secs: Option<u32>,
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            allowed_origins: vec![],
            allowed_methods: vec![
                "GET".into(),
                "HEAD".into(),
                "POST".into(),
                "PUT".into(),
                "PATCH".into(),
                "DELETE".into(),
                "OPTIONS".into(),
            ],
            allowed_headers: vec![
                "Content-Type".into(),
                "Authorization".into(),
                "Accept".into(),
                "Accept-Language".into(),
                "X-Request-Id".into(),
                "X-Correlation-Id".into(),
                "X-Tenant-Id".into(),
            ],
            expose_headers: vec!["X-Request-Id".into()],
            allow_credentials: false,
            max_age_secs: Some(86_400),
        }
    }
}

impl CorsConfig {
    /// Fully permissive (wildcard). **Development only — never production.**
    pub fn permissive() -> Self {
        Self {
            allowed_origins: vec!["*".into()],
            allow_credentials: false,
            ..Self::default()
        }
    }

    /// Explicit origins with credentials enabled — production-ready.
    ///
    /// ```
    /// use klauthed_web::cors::CorsConfig;
    ///
    /// let c = CorsConfig::restrictive(["https://app.example.com"]);
    /// assert!(c.allow_credentials);
    /// ```
    pub fn restrictive(origins: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            allowed_origins: origins.into_iter().map(Into::into).collect(),
            allow_credentials: true,
            ..Self::default()
        }
    }
}

/// Build an [`actix_cors::Cors`] middleware from `config` (static origins).
///
/// For tenant-registered dynamic origins use [`DynamicCors`] instead.
pub fn build_cors(config: &CorsConfig) -> Cors {
    let is_wildcard = config.allowed_origins.iter().any(|o| o == "*");

    let mut cors = if is_wildcard {
        Cors::permissive()
    } else {
        let mut c = Cors::default();
        for origin in &config.allowed_origins {
            c = c.allowed_origin(origin);
        }
        c
    };

    let methods: Vec<actix_web::http::Method> = config
        .allowed_methods
        .iter()
        .filter_map(|m| m.parse().ok())
        .collect();
    if !methods.is_empty() {
        cors = cors.allowed_methods(methods);
    }

    for header in &config.allowed_headers {
        if let Ok(name) = header.parse::<actix_web::http::header::HeaderName>() {
            cors = cors.allowed_header(name);
        }
    }

    let expose: Vec<actix_web::http::header::HeaderName> = config
        .expose_headers
        .iter()
        .filter_map(|h| h.parse().ok())
        .collect();
    if !expose.is_empty() {
        cors = cors.expose_headers(expose);
    }

    if config.allow_credentials {
        cors = cors.supports_credentials();
    }

    if let Some(secs) = config.max_age_secs {
        cors = cors.max_age(secs as usize);
    }

    cors
}

// ── Dynamic CORS — origin registry ───────────────────────────────────────────

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
            if let Some(&(allowed, cached_at)) = cache.get(origin) {
                if cached_at.elapsed() < self.ttl {
                    return allowed;
                }
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

// ── DynamicCors middleware ────────────────────────────────────────────────────

/// Per-request dynamic CORS middleware.
///
/// On each request it inspects the `Origin` header and checks it against:
/// 1. `config.allowed_origins` — your platform's own frontends (O(1), no I/O)
/// 2. A [`CorsOriginRegistry`] — tenant-registered domains (async lookup)
///
/// If the origin is allowed:
/// * `OPTIONS` preflight → short-circuits with `204 No Content` + preflight
///   headers (inner service is never called).
/// * Other methods → calls the inner service, then adds CORS headers to the
///   response.
///
/// If the origin is absent or not allowed the request passes through
/// untouched. `Vary: Origin` is always appended on responses for requests
/// that carry an `Origin` header so HTTP caches handle the variance correctly.
///
/// See the [module-level docs](self) for wiring instructions and security
/// notes.
#[derive(Clone)]
pub struct DynamicCors {
    /// Origins checked without any I/O (e.g. your own auth frontend).
    static_origins: Arc<HashSet<String>>,
    /// Dynamic registry (tenant domains, backed by DB + cache).
    registry: Arc<dyn CorsOriginRegistry>,
    /// CORS policy shared across all allowed responses.
    config: Arc<CorsConfig>,
}

impl DynamicCors {
    /// Build with `config` (policy + static origins) and a `registry` for
    /// tenant domains.
    ///
    /// `config.allowed_origins` becomes the set of always-allowed platform
    /// origins (checked before the registry). The rest of `config` drives the
    /// CORS headers that are sent.
    pub fn new(config: CorsConfig, registry: Arc<dyn CorsOriginRegistry>) -> Self {
        let static_set: HashSet<String> = config.allowed_origins.iter().cloned().collect();
        Self {
            static_origins: Arc::new(static_set),
            registry,
            config: Arc::new(config),
        }
    }

    /// Convenience: use an [`InMemoryOriginRegistry`] (tenant origins added
    /// at runtime via `registry.insert()`).
    pub fn with_memory_registry(
        config: CorsConfig,
    ) -> (Self, Arc<InMemoryOriginRegistry>) {
        let reg = Arc::new(InMemoryOriginRegistry::new());
        let cors = Self::new(config, Arc::clone(&reg) as Arc<dyn CorsOriginRegistry>);
        (cors, reg)
    }
}

impl<S, B> Transform<S, ServiceRequest> for DynamicCors
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: actix_web::body::MessageBody + 'static,
{
    type Response = ServiceResponse<BoxBody>;
    type Error = Error;
    type Transform = DynamicCorsService<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(DynamicCorsService {
            service: Rc::new(service),
            static_origins: Arc::clone(&self.static_origins),
            registry: Arc::clone(&self.registry),
            config: Arc::clone(&self.config),
        }))
    }
}

/// The [`Service`] produced by [`DynamicCors`].
pub struct DynamicCorsService<S> {
    service: Rc<S>,
    static_origins: Arc<HashSet<String>>,
    registry: Arc<dyn CorsOriginRegistry>,
    config: Arc<CorsConfig>,
}

impl<S, B> Service<ServiceRequest> for DynamicCorsService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: actix_web::body::MessageBody + 'static,
{
    type Response = ServiceResponse<BoxBody>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&self, cx: &mut TaskContext<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let static_origins = Arc::clone(&self.static_origins);
        let registry = Arc::clone(&self.registry);
        let config = Arc::clone(&self.config);
        let service = Rc::clone(&self.service);

        // Read the Origin header before moving `req`.
        let origin: Option<String> = req
            .headers()
            .get(ORIGIN)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);

        Box::pin(async move {
            // Determine whether this Origin is allowed (O(1) static check
            // first, async registry only on a miss).
            let allowed_origin: Option<String> = match origin {
                None => None, // No Origin header → same-origin, no CORS needed.
                Some(ref o) => {
                    let allowed = static_origins.contains(o.as_str())
                        || registry.is_allowed(o).await;
                    if allowed { Some(o.clone()) } else { None }
                }
            };

            // CORS preflight: answer OPTIONS immediately, never call inner service.
            if *req.method() == Method::OPTIONS {
                let mut resp = HttpResponse::NoContent()
                    .finish()
                    .map_into_boxed_body();

                // Even for rejected origins we add Vary so caches don't serve
                // a preflight response to a different origin.
                resp.headers_mut()
                    .append(VARY, HeaderValue::from_static("Origin"));

                if let Some(ref o) = allowed_origin {
                    set_cors_headers(resp.headers_mut(), o, &config, true);
                }
                return Ok(req.into_response(resp));
            }

            // Non-preflight: call the inner service.
            let mut res = service
                .call(req)
                .await
                .map(ServiceResponse::map_into_boxed_body)?;

            // Vary: Origin on every response that had an Origin header so
            // caches (CDN, browser) handle origin variance correctly.
            if origin.is_some() {
                res.headers_mut()
                    .append(VARY, HeaderValue::from_static("Origin"));
            }

            if let Some(ref o) = allowed_origin {
                set_cors_headers(res.headers_mut(), o, &config, false);
            }

            Ok(res)
        })
    }
}

/// Write CORS headers onto `headers`. `is_preflight` enables the extra
/// method/header negotiation headers that only belong on `OPTIONS` responses.
fn set_cors_headers(headers: &mut HeaderMap, origin: &str, config: &CorsConfig, is_preflight: bool) {
    // Echo the exact origin — never use `*` with credentials.
    if let Ok(v) = HeaderValue::from_str(origin) {
        headers.insert(ACCESS_CONTROL_ALLOW_ORIGIN, v);
    }

    if config.allow_credentials {
        headers.insert(
            ACCESS_CONTROL_ALLOW_CREDENTIALS,
            HeaderValue::from_static("true"),
        );
    }

    if !config.expose_headers.is_empty() {
        if let Ok(v) = HeaderValue::from_str(&config.expose_headers.join(", ")) {
            headers.insert(ACCESS_CONTROL_EXPOSE_HEADERS, v);
        }
    }

    // Preflight-only: method + header negotiation and cache lifetime.
    if is_preflight {
        if let Ok(v) = HeaderValue::from_str(&config.allowed_methods.join(", ")) {
            headers.insert(ACCESS_CONTROL_ALLOW_METHODS, v);
        }
        if let Ok(v) = HeaderValue::from_str(&config.allowed_headers.join(", ")) {
            headers.insert(ACCESS_CONTROL_ALLOW_HEADERS, v);
        }
        if let Some(secs) = config.max_age_secs {
            if let Ok(v) = HeaderValue::from_str(&secs.to_string()) {
                headers.insert(ACCESS_CONTROL_MAX_AGE, v);
            }
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::http::StatusCode;
    use actix_web::test as http_test;
    use actix_web::{web, App, HttpResponse};

    // ── Static CORS tests ─────────────────────────────────────────────────────

    #[test]
    fn permissive_has_wildcard_and_no_credentials() {
        let c = CorsConfig::permissive();
        assert!(c.allowed_origins.iter().any(|o| o == "*"));
        assert!(!c.allow_credentials);
    }

    #[test]
    fn restrictive_sets_origins_and_credentials() {
        let c = CorsConfig::restrictive(["https://app.example.com"]);
        assert_eq!(c.allowed_origins, ["https://app.example.com"]);
        assert!(c.allow_credentials);
    }

    #[test]
    fn build_cors_does_not_panic_for_permissive() {
        let _ = build_cors(&CorsConfig::permissive());
    }

    #[test]
    fn build_cors_does_not_panic_for_default() {
        let _ = build_cors(&CorsConfig::default());
    }

    #[test]
    fn build_cors_does_not_panic_for_restrictive() {
        let _ = build_cors(&CorsConfig::restrictive(["https://a.example.com"]));
    }

    #[test]
    fn config_serde_round_trips() {
        let config = CorsConfig::restrictive(["https://a.example.com"]);
        let json = serde_json::to_string(&config).unwrap();
        let back: CorsConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.allowed_origins, config.allowed_origins);
        assert_eq!(back.allow_credentials, config.allow_credentials);
    }

    // ── InMemoryOriginRegistry tests ──────────────────────────────────────────

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

    // ── CachedOriginRegistry tests ────────────────────────────────────────────

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

    // ── DynamicCors middleware tests ──────────────────────────────────────────

    fn platform_config() -> CorsConfig {
        CorsConfig {
            allowed_origins: vec!["https://auth.platform.com".into()],
            allow_credentials: true,
            ..CorsConfig::default()
        }
    }

    async fn ok_handler() -> HttpResponse {
        HttpResponse::Ok().body("hello")
    }

    #[actix_web::test]
    async fn static_platform_origin_is_allowed_without_registry() {
        let reg = Arc::new(InMemoryOriginRegistry::new()); // empty registry
        let cors = DynamicCors::new(platform_config(), reg);
        let app = http_test::init_service(
            App::new().wrap(cors).route("/", web::get().to(ok_handler)),
        )
        .await;

        let req = http_test::TestRequest::get()
            .uri("/")
            .insert_header(("Origin", "https://auth.platform.com"))
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let acao = resp
            .headers()
            .get("access-control-allow-origin")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert_eq!(acao, "https://auth.platform.com");

        let vary = resp
            .headers()
            .get("vary")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(vary.contains("Origin"));
    }

    #[actix_web::test]
    async fn tenant_origin_from_registry_is_allowed() {
        let reg = Arc::new(InMemoryOriginRegistry::new());
        reg.insert("https://login.acme.com");
        let cors = DynamicCors::new(platform_config(), Arc::clone(&reg) as Arc<dyn CorsOriginRegistry>);
        let app = http_test::init_service(
            App::new().wrap(cors).route("/", web::get().to(ok_handler)),
        )
        .await;

        let req = http_test::TestRequest::get()
            .uri("/")
            .insert_header(("Origin", "https://login.acme.com"))
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers()
                .get("access-control-allow-origin")
                .and_then(|v| v.to_str().ok()),
            Some("https://login.acme.com")
        );
    }

    #[actix_web::test]
    async fn unknown_origin_gets_no_cors_headers() {
        let reg = Arc::new(InMemoryOriginRegistry::new());
        let cors = DynamicCors::new(platform_config(), reg);
        let app = http_test::init_service(
            App::new().wrap(cors).route("/", web::get().to(ok_handler)),
        )
        .await;

        let req = http_test::TestRequest::get()
            .uri("/")
            .insert_header(("Origin", "https://attacker.example.com"))
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK); // request succeeds (CORS is client-enforced)
        assert!(resp.headers().get("access-control-allow-origin").is_none());
        // Vary: Origin still set so caches handle it correctly.
        let vary = resp.headers().get("vary").and_then(|v| v.to_str().ok()).unwrap_or("");
        assert!(vary.contains("Origin"));
    }

    #[actix_web::test]
    async fn options_preflight_for_allowed_origin_returns_204() {
        let reg = Arc::new(InMemoryOriginRegistry::with_origins(["https://login.acme.com"]));
        let cors = DynamicCors::new(platform_config(), reg);
        let app = http_test::init_service(
            App::new().wrap(cors).route("/api", web::get().to(ok_handler)),
        )
        .await;

        let req = http_test::TestRequest::default()
            .method(Method::OPTIONS)
            .uri("/api")
            .insert_header(("Origin", "https://login.acme.com"))
            .insert_header(("Access-Control-Request-Method", "POST"))
            .insert_header(("Access-Control-Request-Headers", "content-type,authorization"))
            .to_request();
        let resp = http_test::call_service(&app, req).await;

        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            resp.headers()
                .get("access-control-allow-origin")
                .and_then(|v| v.to_str().ok()),
            Some("https://login.acme.com")
        );
        // Preflight must include methods and headers.
        assert!(resp.headers().contains_key("access-control-allow-methods"));
        assert!(resp.headers().contains_key("access-control-allow-headers"));
    }

    #[actix_web::test]
    async fn options_preflight_for_unknown_origin_passes_through_with_vary() {
        let reg = Arc::new(InMemoryOriginRegistry::new());
        let cors = DynamicCors::new(platform_config(), reg);
        let app = http_test::init_service(
            App::new().wrap(cors).route("/", web::get().to(ok_handler)),
        )
        .await;

        let req = http_test::TestRequest::default()
            .method(Method::OPTIONS)
            .uri("/")
            .insert_header(("Origin", "https://unknown.example.com"))
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        // Rejected origin → 204 with Vary but no ACAO header.
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        assert!(resp.headers().get("access-control-allow-origin").is_none());
        let vary = resp.headers().get("vary").and_then(|v| v.to_str().ok()).unwrap_or("");
        assert!(vary.contains("Origin"));
    }

    #[actix_web::test]
    async fn no_origin_header_passes_through_cleanly() {
        let reg = Arc::new(InMemoryOriginRegistry::new());
        let cors = DynamicCors::new(platform_config(), reg);
        let app = http_test::init_service(
            App::new().wrap(cors).route("/", web::get().to(ok_handler)),
        )
        .await;

        let req = http_test::TestRequest::get().uri("/").to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(resp.headers().get("access-control-allow-origin").is_none());
        // No Origin header → no Vary either.
        assert!(!resp.headers().contains_key("vary"));
    }

    #[actix_web::test]
    async fn tenant_added_at_runtime_is_allowed_immediately() {
        let (cors, reg) = DynamicCors::with_memory_registry(platform_config());
        let app = http_test::init_service(
            App::new().wrap(cors).route("/", web::get().to(ok_handler)),
        )
        .await;

        // Before registration → rejected.
        let req = http_test::TestRequest::get()
            .uri("/")
            .insert_header(("Origin", "https://login.new-tenant.com"))
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        assert!(resp.headers().get("access-control-allow-origin").is_none());

        // Register the domain at runtime.
        reg.insert("https://login.new-tenant.com");

        // After registration → allowed.
        let req = http_test::TestRequest::get()
            .uri("/")
            .insert_header(("Origin", "https://login.new-tenant.com"))
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(
            resp.headers()
                .get("access-control-allow-origin")
                .and_then(|v| v.to_str().ok()),
            Some("https://login.new-tenant.com")
        );
    }

    #[test]
    fn with_memory_registry_convenience_returns_shared_handle() {
        let (_, reg) = DynamicCors::with_memory_registry(CorsConfig::default());
        reg.insert("https://test.example.com");
        assert_eq!(reg.len(), 1);
    }
}
