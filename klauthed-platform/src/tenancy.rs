//! Multi-tenant primitives.
//!
//! A [`Tenant`] is the unit of isolation in the platform. It is identified by a
//! typed [`TenantId`] (an [`Id<Tenant>`](klauthed_core::id::Id)) and addressed
//! operationally by a human-readable [`slug`](Tenant::slug). A [`TenantResolver`]
//! turns an id *or* slug into a [`Tenant`]; [`InMemoryTenantResolver`] is a
//! deterministic implementation for tests and local development.
//!
//! ```
//! use klauthed_platform::tenancy::{Tenant, TenantStatus};
//!
//! let t = Tenant::new("acme").with_name("Acme, Inc.");
//! assert_eq!(t.slug(), "acme");
//! assert_eq!(t.status(), TenantStatus::Active);
//! assert!(t.is_active());
//! ```

use std::collections::BTreeMap;
use std::sync::Mutex;

use async_trait::async_trait;
use klauthed_core::context::RequestContext;
use klauthed_core::id::Id;
use klauthed_core::time::{Clock, SystemClock, Timestamp};
use serde::{Deserialize, Serialize};

use crate::error::PlatformError;

/// A typed, time-sortable tenant identifier ([`Id<Tenant>`](Id)).
///
/// The [`Tenant`] struct itself serves as the phantom tag for [`Id`], so
/// `TenantId` is distinct from every other crate's id type at compile time.
pub type TenantId = Id<Tenant>;

/// Lifecycle state of a [`Tenant`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TenantStatus {
    /// Fully provisioned and able to serve traffic.
    Active,
    /// Created but not yet activated (e.g. awaiting verification).
    Pending,
    /// Temporarily disabled; access should be refused.
    Suspended,
}

impl TenantStatus {
    /// Whether this status permits serving traffic.
    pub fn is_active(self) -> bool {
        matches!(self, TenantStatus::Active)
    }
}

/// A tenant: the unit of isolation in the platform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tenant {
    id: TenantId,
    slug: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    status: TenantStatus,
    created_at: Timestamp,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    metadata: BTreeMap<String, String>,
}

impl Tenant {
    /// A new active tenant with a fresh id and `created_at` set to now.
    pub fn new(slug: impl Into<String>) -> Self {
        Self::new_at(slug, &SystemClock)
    }

    /// Like [`new`](Tenant::new) but takes `created_at` from `clock`, for tests.
    pub fn new_at(slug: impl Into<String>, clock: &impl Clock) -> Self {
        Self {
            id: TenantId::new(),
            slug: slug.into(),
            name: None,
            status: TenantStatus::Active,
            created_at: clock.now(),
            metadata: BTreeMap::new(),
        }
    }

    // ── Builders ──────────────────────────────────────────────────────────────

    /// Override the id (e.g. when rehydrating from storage).
    pub fn with_id(mut self, id: TenantId) -> Self {
        self.id = id;
        self
    }

    /// Set the display name.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the lifecycle status.
    pub fn with_status(mut self, status: TenantStatus) -> Self {
        self.status = status;
        self
    }

    /// Set the creation timestamp.
    pub fn with_created_at(mut self, at: Timestamp) -> Self {
        self.created_at = at;
        self
    }

    /// Insert a metadata entry (builder form).
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    // ── Accessors ─────────────────────────────────────────────────────────────

    /// The typed tenant id.
    pub fn id(&self) -> TenantId {
        self.id
    }

    /// The operational slug.
    pub fn slug(&self) -> &str {
        &self.slug
    }

    /// The display name, if set.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// The lifecycle status.
    pub fn status(&self) -> TenantStatus {
        self.status
    }

    /// When the tenant was created.
    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }

    /// All metadata.
    pub fn metadata(&self) -> &BTreeMap<String, String> {
        &self.metadata
    }

    /// A single metadata value.
    pub fn metadata_get(&self, key: &str) -> Option<&str> {
        self.metadata.get(key).map(String::as_str)
    }

    /// Whether the tenant is [`Active`](TenantStatus::Active).
    pub fn is_active(&self) -> bool {
        self.status.is_active()
    }

    /// `Ok(self)` if active, else [`PlatformError::TenantSuspended`].
    ///
    /// Use at access-control boundaries to reject suspended/pending tenants.
    pub fn ensure_active(&self) -> Result<&Self, PlatformError> {
        if self.is_active() {
            Ok(self)
        } else {
            Err(PlatformError::TenantSuspended { slug: self.slug.clone() })
        }
    }
}

/// Resolves a tenant from an id-or-slug string.
///
/// Implementors are `Send + Sync` so a resolver can be shared as
/// `Arc<dyn TenantResolver>` across tasks. `resolve` returns `Ok(None)` when no
/// tenant matches (a normal, non-error outcome); see [`require`](TenantResolver::require)
/// for the not-found-is-an-error variant.
#[async_trait]
pub trait TenantResolver: Send + Sync {
    /// Look up a tenant by its [`TenantId`] (UUID/ULID string) or [`slug`](Tenant::slug).
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

#[cfg(test)]
mod tests {
    use super::*;
    use klauthed_error::{DomainError, ErrorCategory};

    fn acme() -> Tenant {
        Tenant::new("acme").with_name("Acme, Inc.")
    }

    #[test]
    fn tenant_builder_and_accessors() {
        let t = acme().with_metadata("plan", "pro");
        assert_eq!(t.slug(), "acme");
        assert_eq!(t.name(), Some("Acme, Inc."));
        assert_eq!(t.status(), TenantStatus::Active);
        assert!(t.is_active());
        assert_eq!(t.metadata_get("plan"), Some("pro"));
    }

    #[test]
    fn ensure_active_rejects_suspended() {
        let t = acme().with_status(TenantStatus::Suspended);
        let err = t.ensure_active().unwrap_err();
        assert_eq!(err.category(), ErrorCategory::Forbidden);
        assert_eq!(err.code().as_str(), "platform.tenant_suspended");
    }

    #[test]
    fn status_serde_is_snake_case() {
        let json = serde_json::to_string(&TenantStatus::Suspended).unwrap();
        assert_eq!(json, "\"suspended\"");
        let back: TenantStatus = serde_json::from_str("\"pending\"").unwrap();
        assert_eq!(back, TenantStatus::Pending);
    }

    #[test]
    fn tenant_round_trips_through_json() {
        let t = acme().with_metadata("k", "v");
        let json = serde_json::to_string(&t).unwrap();
        let back: Tenant = serde_json::from_str(&json).unwrap();
        assert_eq!(t, back);
    }

    #[tokio::test]
    async fn in_memory_resolver_by_id_and_slug() {
        let t = acme();
        let id = t.id();
        let resolver = InMemoryTenantResolver::with_tenants([t]);
        assert_eq!(resolver.len(), 1);

        let by_slug = resolver.resolve("acme").await.unwrap();
        assert_eq!(by_slug.as_ref().map(Tenant::id), Some(id));

        let by_id = resolver.resolve(&id.to_string()).await.unwrap();
        assert_eq!(by_id.map(|t| t.id()), Some(id));

        assert!(resolver.resolve("nope").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn require_maps_miss_to_not_found() {
        let resolver = InMemoryTenantResolver::new();
        let err = resolver.require("ghost").await.unwrap_err();
        assert_eq!(err.category(), ErrorCategory::NotFound);
        assert_eq!(err.code().as_str(), "platform.tenant_not_found");
    }

    #[tokio::test]
    async fn from_context_uses_ctx_tenant() {
        let resolver = InMemoryTenantResolver::with_tenants([acme()]);

        let ctx = RequestContext::new().with_tenant("acme");
        let resolved = tenant_from_context(&resolver, &ctx).await.unwrap();
        assert_eq!(resolved.map(|t| t.slug().to_owned()), Some("acme".into()));

        let ctx = RequestContext::new();
        assert!(tenant_from_context(&resolver, &ctx).await.unwrap().is_none());
    }
}
