#![deny(unsafe_code)]

//! Per-request execution context.
//!
//! [`RequestContext`] carries the cross-cutting facts about the work in flight —
//! a generated request id, an inbound correlation id, tenant and principal,
//! locale, when it arrived, an optional deadline, and a free-form metadata bag.
//!
//! There are two ways to use it, by design:
//!
//! * **Explicit** (always available): construct a `RequestContext` and pass
//!   `&ctx` down the call chain. This is the source of truth — clear and testable.
//! * **Ambient** (feature `task-local`): set the context once for a request with
//!   [`RequestContext::scope`] and read it anywhere below with
//!   [`RequestContext::try_current`], without threading it through every signature.
//!
//! ```
//! use klauthed_core::context::RequestContext;
//!
//! let ctx = RequestContext::new()
//!     .with_correlation_id("trace-abc")
//!     .with_tenant("acme")
//!     .with_metadata("feature_flag", "beta");
//!
//! assert!(ctx.correlation_id().is_some());
//! assert_eq!(ctx.tenant(), Some("acme"));
//! assert_eq!(ctx.metadata_get("feature_flag"), Some("beta"));
//! ```

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::time::Duration;

use crate::id::Id;
use crate::time::Timestamp;

/// Marker tag for a request identifier.
pub struct Request;

/// The id minted for each incoming request.
pub type RequestId = Id<Request>;

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
    pub fn with_request_id(mut self, id: RequestId) -> Self {
        self.request_id = id;
        self
    }

    /// Set the inbound correlation / trace id.
    pub fn with_correlation_id(mut self, id: impl Into<String>) -> Self {
        self.correlation_id = Some(id.into());
        self
    }

    /// Set the tenant identifier.
    pub fn with_tenant(mut self, tenant: impl Into<String>) -> Self {
        self.tenant = Some(tenant.into());
        self
    }

    /// Set the authenticated principal / subject.
    pub fn with_principal(mut self, principal: impl Into<String>) -> Self {
        self.principal = Some(principal.into());
        self
    }

    /// Set the preferred locale (BCP-47, e.g. `en-US`).
    pub fn with_locale(mut self, locale: impl Into<String>) -> Self {
        self.locale = Some(locale.into());
        self
    }

    /// Set the arrival time (defaults to construction time).
    pub fn with_received_at(mut self, at: Timestamp) -> Self {
        self.received_at = at;
        self
    }

    /// Set an absolute deadline for the work.
    pub fn with_deadline(mut self, deadline: Timestamp) -> Self {
        self.deadline = Some(deadline);
        self
    }

    /// Insert a metadata entry (builder form).
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

// ── Ambient (task-local) propagation ──────────────────────────────────────────

#[cfg(feature = "task-local")]
mod ambient {
    use std::future::Future;

    use super::RequestContext;

    tokio::task_local! {
        static CURRENT: RequestContext;
    }

    impl RequestContext {
        /// Run `future` with this context installed as the current one, so code
        /// below can read it via [`try_current`](RequestContext::try_current)
        /// without it being passed explicitly.
        pub async fn scope<F>(self, future: F) -> F::Output
        where
            F: Future,
        {
            CURRENT.scope(self, future).await
        }

        /// A clone of the current context, or `None` if called outside a
        /// [`scope`](RequestContext::scope).
        pub fn try_current() -> Option<RequestContext> {
            CURRENT.try_with(|ctx| ctx.clone()).ok()
        }

        /// Borrow the current context to compute a value, or `None` if called
        /// outside a [`scope`](RequestContext::scope).
        pub fn with_current<R>(f: impl FnOnce(&RequestContext) -> R) -> Option<R> {
            CURRENT.try_with(f).ok()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_with_fields_and_unique_request_id() {
        let a = RequestContext::new();
        let b = RequestContext::new();
        assert_ne!(a.request_id(), b.request_id());

        let ctx = RequestContext::new()
            .with_correlation_id("corr-1")
            .with_tenant("acme")
            .with_principal("user-42")
            .with_locale("tr-TR")
            .with_metadata("k", "v");

        assert_eq!(ctx.correlation_id(), Some("corr-1"));
        assert_eq!(ctx.tenant(), Some("acme"));
        assert_eq!(ctx.principal(), Some("user-42"));
        assert_eq!(ctx.locale(), Some("tr-TR"));
        assert_eq!(ctx.metadata_get("k"), Some("v"));
    }

    #[test]
    fn deadline_helpers() {
        let start = Timestamp::from_unix_millis(10_000);
        let ctx = RequestContext::new()
            .with_received_at(start)
            .with_deadline(Timestamp::from_unix_millis(15_000));

        let before = Timestamp::from_unix_millis(12_000);
        let after = Timestamp::from_unix_millis(16_000);

        assert!(!ctx.is_expired(before));
        assert!(ctx.is_expired(after));
        assert_eq!(ctx.time_remaining(before).unwrap().whole_seconds(), 3);
        assert_eq!(ctx.age(before).whole_seconds(), 2);
    }

    #[test]
    fn serde_round_trip_skips_empty_fields() {
        let ctx = RequestContext::new().with_tenant("acme");
        let json = serde_json::to_string(&ctx).unwrap();
        assert!(json.contains("\"tenant\":\"acme\""));
        assert!(!json.contains("correlation_id"));
        let back: RequestContext = serde_json::from_str(&json).unwrap();
        assert_eq!(back.request_id(), ctx.request_id());
        assert_eq!(back.tenant(), Some("acme"));
    }

    #[cfg(feature = "task-local")]
    #[tokio::test]
    async fn ambient_context_is_readable_within_scope() {
        assert!(RequestContext::try_current().is_none());

        let ctx = RequestContext::new().with_tenant("acme");
        let id = ctx.request_id();

        ctx.scope(async move {
            let current = RequestContext::try_current().expect("context in scope");
            assert_eq!(current.request_id(), id);
            assert_eq!(current.tenant(), Some("acme"));
            let tenant = RequestContext::with_current(|c| c.tenant().map(str::to_owned)).flatten();
            assert_eq!(tenant.as_deref(), Some("acme"));
        })
        .await;

        assert!(RequestContext::try_current().is_none());
    }
}
