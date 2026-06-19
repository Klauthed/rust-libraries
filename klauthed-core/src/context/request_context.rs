//! The [`RequestContext`] value type.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::RequestId;
use crate::time::{Duration, Timestamp};

/// Cross-cutting context for a single unit of work (typically one request).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestContext {
    request_id: RequestId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    correlation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tenant: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    principal: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    locale: Option<String>,
    received_at: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    deadline: Option<Timestamp>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    metadata: BTreeMap<String, String>,
}

impl RequestContext {
    /// A fresh context: a new request id and `received_at` set to now.
    pub fn new() -> Self {
        Self {
            request_id: RequestId::new(),
            correlation_id: None,
            tenant: None,
            principal: None,
            locale: None,
            received_at: Timestamp::now(),
            deadline: None,
            metadata: BTreeMap::new(),
        }
    }

    // ── Builders ──────────────────────────────────────────────────────────────

    /// Override the request id (e.g. when reconstructing a propagated context).
    #[must_use]
    pub fn with_request_id(mut self, id: RequestId) -> Self {
        self.request_id = id;
        self
    }

    /// Set the inbound correlation / trace id.
    #[must_use]
    pub fn with_correlation_id(mut self, id: impl Into<String>) -> Self {
        self.correlation_id = Some(id.into());
        self
    }

    /// Set the tenant identifier.
    #[must_use]
    pub fn with_tenant(mut self, tenant: impl Into<String>) -> Self {
        self.tenant = Some(tenant.into());
        self
    }

    /// Set the authenticated principal / subject.
    #[must_use]
    pub fn with_principal(mut self, principal: impl Into<String>) -> Self {
        self.principal = Some(principal.into());
        self
    }

    /// Set the preferred locale (BCP-47, e.g. `en-US`).
    #[must_use]
    pub fn with_locale(mut self, locale: impl Into<String>) -> Self {
        self.locale = Some(locale.into());
        self
    }

    /// Set the arrival time (defaults to construction time).
    #[must_use]
    pub fn with_received_at(mut self, at: Timestamp) -> Self {
        self.received_at = at;
        self
    }

    /// Set an absolute deadline for the work.
    #[must_use]
    pub fn with_deadline(mut self, deadline: Timestamp) -> Self {
        self.deadline = Some(deadline);
        self
    }

    /// Insert a metadata entry (builder form).
    #[must_use]
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Insert a metadata entry in place.
    pub fn insert_metadata(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.metadata.insert(key.into(), value.into());
    }

    // ── Accessors ─────────────────────────────────────────────────────────────

    /// The request id.
    pub fn request_id(&self) -> RequestId {
        self.request_id
    }

    /// The inbound correlation / trace id, if any.
    pub fn correlation_id(&self) -> Option<&str> {
        self.correlation_id.as_deref()
    }

    /// The tenant identifier, if any.
    pub fn tenant(&self) -> Option<&str> {
        self.tenant.as_deref()
    }

    /// The principal / subject, if any.
    pub fn principal(&self) -> Option<&str> {
        self.principal.as_deref()
    }

    /// The preferred locale, if any.
    pub fn locale(&self) -> Option<&str> {
        self.locale.as_deref()
    }

    /// When the work arrived.
    pub fn received_at(&self) -> Timestamp {
        self.received_at
    }

    /// The deadline, if set.
    pub fn deadline(&self) -> Option<Timestamp> {
        self.deadline
    }

    /// All metadata.
    pub fn metadata(&self) -> &BTreeMap<String, String> {
        &self.metadata
    }

    /// A single metadata value.
    pub fn metadata_get(&self, key: &str) -> Option<&str> {
        self.metadata.get(key).map(String::as_str)
    }

    // ── Deadline helpers ──────────────────────────────────────────────────────

    /// How long since the work arrived, as of `now`.
    pub fn age(&self, now: Timestamp) -> Duration {
        now.duration_since(self.received_at)
    }

    /// Whether the deadline (if any) has passed as of `now`.
    pub fn is_expired(&self, now: Timestamp) -> bool {
        self.deadline.is_some_and(|d| now >= d)
    }

    /// Time left until the deadline as of `now` (`None` if no deadline).
    pub fn time_remaining(&self, now: Timestamp) -> Option<Duration> {
        self.deadline.map(|d| d.duration_since(now))
    }
}

impl Default for RequestContext {
    fn default() -> Self {
        Self::new()
    }
}
