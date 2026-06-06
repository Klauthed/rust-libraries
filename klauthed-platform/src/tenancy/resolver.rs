//! Tenant resolution: the [`TenantResolver`] trait, [`tenant_from_context`],
//! and the in-memory [`InMemoryTenantResolver`].

use std::sync::Mutex;

use async_trait::async_trait;
use klauthed_core::context::RequestContext;

use crate::error::PlatformError;

use super::Tenant;

/// Resolves a tenant from an id-or-slug string.
///
/// Implementors are `Send + Sync` so a resolver can be shared as
/// `Arc<dyn TenantResolver>` across tasks. `resolve` returns `Ok(None)` when no
/// tenant matches (a normal, non-error outcome); see [`require`](TenantResolver::require)
/// for the not-found-is-an-error variant.
#[async_trait]
pub trait TenantResolver: Send + Sync {
    /// Look up a tenant by its [`TenantId`](super::TenantId) (UUID/ULID string) or [`slug`](Tenant::slug).
    async fn resolve(&self, id_or_slug: &str) -> Result<Option<Tenant>, PlatformError>;

    /// Like [`resolve`](TenantResolver::resolve) but maps a miss to
    /// [`PlatformError::TenantNotFound`].
    async fn require(&self, id_or_slug: &str) -> Result<Tenant, PlatformError> {
        match self.resolve(id_or_slug).await? {
            Some(tenant) => Ok(tenant),
            None => Err(PlatformError::TenantNotFound { id_or_slug: id_or_slug.to_owned() }),
        }
    }
}

/// Read the active tenant for a [`RequestContext`] via a [`TenantResolver`].
///
/// Returns `Ok(None)` when the context carries no tenant; otherwise resolves the
/// context's [`tenant`](RequestContext::tenant) value (treated as an id or slug).
pub async fn tenant_from_context(
    resolver: &impl TenantResolver,
    ctx: &RequestContext,
) -> Result<Option<Tenant>, PlatformError> {
    match ctx.tenant() {
        Some(id_or_slug) => resolver.resolve(id_or_slug).await,
        None => Ok(None),
    }
}

/// An in-memory [`TenantResolver`] for tests and local development.
///
/// Tenants are indexed by both id (string form) and slug, so either resolves.
#[derive(Default)]
pub struct InMemoryTenantResolver {
    tenants: Mutex<Vec<Tenant>>,
}

impl InMemoryTenantResolver {
    /// An empty resolver.
    pub fn new() -> Self {
        Self::default()
    }

    /// Build from an iterator of tenants.
    pub fn with_tenants(tenants: impl IntoIterator<Item = Tenant>) -> Self {
        Self { tenants: Mutex::new(tenants.into_iter().collect()) }
    }

    /// Insert (or replace, by id) a tenant.
    pub fn insert(&self, tenant: Tenant) {
        let mut guard = self.tenants.lock().expect("tenant lock poisoned");
        if let Some(slot) = guard.iter_mut().find(|t| t.id() == tenant.id()) {
            *slot = tenant;
        } else {
            guard.push(tenant);
        }
    }

    /// The number of registered tenants.
    pub fn len(&self) -> usize {
        self.tenants.lock().expect("tenant lock poisoned").len()
    }

    /// Whether there are no registered tenants.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[async_trait]
impl TenantResolver for InMemoryTenantResolver {
    async fn resolve(&self, id_or_slug: &str) -> Result<Option<Tenant>, PlatformError> {
        let guard = self.tenants.lock().expect("tenant lock poisoned");
        let found = guard
            .iter()
            .find(|t| t.slug() == id_or_slug || t.id().to_string() == id_or_slug)
            .cloned();
        Ok(found)
    }
}
