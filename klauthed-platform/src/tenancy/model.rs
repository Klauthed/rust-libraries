//! The tenant model: [`Tenant`], its typed [`TenantId`], and [`TenantStatus`].

use std::collections::BTreeMap;

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
    #[must_use]
    pub fn with_id(mut self, id: TenantId) -> Self {
        self.id = id;
        self
    }

    /// Set the display name.
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the lifecycle status.
    #[must_use]
    pub fn with_status(mut self, status: TenantStatus) -> Self {
        self.status = status;
        self
    }

    /// Set the creation timestamp.
    #[must_use]
    pub fn with_created_at(mut self, at: Timestamp) -> Self {
        self.created_at = at;
        self
    }

    /// Insert a metadata entry (builder form).
    #[must_use]
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
