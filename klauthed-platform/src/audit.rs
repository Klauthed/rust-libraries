//! Audit events and sinks.
//!
//! An [`AuditEvent`] is an immutable record of a security- or compliance-relevant
//! action: who did what, to which resource, with what outcome, and when. Build
//! one with [`AuditEvent::builder`], then hand it to an [`AuditSink`].
//! [`InMemoryAuditSink`] retains events so tests can assert on them.
//!
//! ```
//! use klauthed_platform::audit::{AuditEvent, AuditOutcome};
//!
//! let event = AuditEvent::builder("tenant.suspend")
//!     .actor("admin-1")
//!     .tenant("acme")
//!     .resource("tenant", "acme")
//!     .outcome(AuditOutcome::Success)
//!     .metadata("reason", "non-payment")
//!     .build();
//!
//! assert_eq!(event.action(), "tenant.suspend");
//! assert!(event.outcome().is_success());
//! ```

use std::collections::BTreeMap;
use std::sync::Mutex;

use async_trait::async_trait;
use klauthed_core::context::RequestContext;
use klauthed_core::id::Id;
use klauthed_core::time::{Clock, SystemClock, Timestamp};
use serde::{Deserialize, Serialize};

use crate::error::PlatformError;

/// Zero-sized marker tagging an [`AuditId`].
pub struct Audit;

/// A typed, time-sortable audit-event identifier.
pub type AuditId = Id<Audit>;

/// The result of the audited action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditOutcome {
    /// The action completed successfully.
    Success,
    /// The action was attempted but failed or was denied.
    Failure,
}

impl AuditOutcome {
    /// Whether this is [`Success`](AuditOutcome::Success).
    pub fn is_success(self) -> bool {
        matches!(self, AuditOutcome::Success)
    }
}

/// An immutable record of an audited action.
///
/// Construct via [`AuditEvent::builder`]; fields are read-only afterward.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEvent {
    id: AuditId,
    occurred_at: Timestamp,
    action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    actor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tenant: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    resource_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    resource_id: Option<String>,
    outcome: AuditOutcome,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    metadata: BTreeMap<String, String>,
}

impl AuditEvent {
    /// Start building an event for `action` (e.g. `tenant.suspend`).
    pub fn builder(action: impl Into<String>) -> AuditEventBuilder {
        AuditEventBuilder::new(action)
    }

    /// The event id.
    pub fn id(&self) -> AuditId {
        self.id
    }

    /// When the action occurred.
    pub fn occurred_at(&self) -> Timestamp {
        self.occurred_at
    }

    /// The action name.
    pub fn action(&self) -> &str {
        &self.action
    }

    /// The actor / principal who performed the action, if known.
    pub fn actor(&self) -> Option<&str> {
        self.actor.as_deref()
    }

    /// The tenant the action applied to, if any.
    pub fn tenant(&self) -> Option<&str> {
        self.tenant.as_deref()
    }

    /// The affected resource type, if any.
    pub fn resource_type(&self) -> Option<&str> {
        self.resource_type.as_deref()
    }

    /// The affected resource id, if any.
    pub fn resource_id(&self) -> Option<&str> {
        self.resource_id.as_deref()
    }

    /// The outcome.
    pub fn outcome(&self) -> AuditOutcome {
        self.outcome
    }

    /// All metadata.
    pub fn metadata(&self) -> &BTreeMap<String, String> {
        &self.metadata
    }
}

/// Ergonomic builder for [`AuditEvent`].
///
/// Defaults: a fresh [`AuditId`], `occurred_at` from the supplied [`Clock`] (or
/// the system clock via [`build`](AuditEventBuilder::build)), and
/// [`AuditOutcome::Success`].
#[derive(Debug, Clone)]
pub struct AuditEventBuilder {
    id: AuditId,
    occurred_at: Option<Timestamp>,
    action: String,
    actor: Option<String>,
    tenant: Option<String>,
    resource_type: Option<String>,
    resource_id: Option<String>,
    outcome: AuditOutcome,
    metadata: BTreeMap<String, String>,
}

impl AuditEventBuilder {
    fn new(action: impl Into<String>) -> Self {
        Self {
            id: AuditId::new(),
            occurred_at: None,
            action: action.into(),
            actor: None,
            tenant: None,
            resource_type: None,
            resource_id: None,
            outcome: AuditOutcome::Success,
            metadata: BTreeMap::new(),
        }
    }

    /// Override the event id.
    pub fn id(mut self, id: AuditId) -> Self {
        self.id = id;
        self
    }

    /// Set an explicit occurrence time (otherwise taken from the clock at build).
    pub fn occurred_at(mut self, at: Timestamp) -> Self {
        self.occurred_at = Some(at);
        self
    }

    /// Set the actor / principal.
    pub fn actor(mut self, actor: impl Into<String>) -> Self {
        self.actor = Some(actor.into());
        self
    }

    /// Set the tenant.
    pub fn tenant(mut self, tenant: impl Into<String>) -> Self {
        self.tenant = Some(tenant.into());
        self
    }

    /// Set the affected resource (type and id together).
    pub fn resource(mut self, ty: impl Into<String>, id: impl Into<String>) -> Self {
        self.resource_type = Some(ty.into());
        self.resource_id = Some(id.into());
        self
    }

    /// Set the outcome (default [`Success`](AuditOutcome::Success)).
    pub fn outcome(mut self, outcome: AuditOutcome) -> Self {
        self.outcome = outcome;
        self
    }

    /// Mark the outcome as [`Failure`](AuditOutcome::Failure).
    pub fn failed(self) -> Self {
        self.outcome(AuditOutcome::Failure)
    }

    /// Insert a metadata entry.
    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Copy `actor`, `tenant`, and the request-id metadata from a
    /// [`RequestContext`] (without overwriting values already set).
    pub fn from_context(mut self, ctx: &RequestContext) -> Self {
        if self.actor.is_none() {
            self.actor = ctx.principal().map(str::to_owned);
        }
        if self.tenant.is_none() {
            self.tenant = ctx.tenant().map(str::to_owned);
        }
        self.metadata
            .entry("request_id".to_owned())
            .or_insert_with(|| ctx.request_id().to_string());
        self
    }

    /// Finish building, stamping `occurred_at` from the system clock if unset.
    pub fn build(self) -> AuditEvent {
        self.build_with(&SystemClock)
    }

    /// Finish building, taking `occurred_at` from `clock` when unset (for tests).
    pub fn build_with(self, clock: &impl Clock) -> AuditEvent {
        AuditEvent {
            id: self.id,
            occurred_at: self.occurred_at.unwrap_or_else(|| clock.now()),
            action: self.action,
            actor: self.actor,
            tenant: self.tenant,
            resource_type: self.resource_type,
            resource_id: self.resource_id,
            outcome: self.outcome,
            metadata: self.metadata,
        }
    }
}

/// A destination for [`AuditEvent`]s.
///
/// Implementors are `Send + Sync` so a sink can be shared as `Arc<dyn AuditSink>`.
#[async_trait]
pub trait AuditSink: Send + Sync {
    /// Persist or forward an event. Returns [`PlatformError::Backend`] on failure.
    async fn record(&self, event: AuditEvent) -> Result<(), PlatformError>;
}

/// An in-memory [`AuditSink`] that retains every recorded event for assertions.
#[derive(Default)]
pub struct InMemoryAuditSink {
    events: Mutex<Vec<AuditEvent>>,
}

impl InMemoryAuditSink {
    /// An empty sink.
    pub fn new() -> Self {
        Self::default()
    }

    /// A snapshot of all recorded events, in record order.
    pub fn events(&self) -> Vec<AuditEvent> {
        self.events.lock().expect("audit lock poisoned").clone()
    }

    /// The number of recorded events.
    pub fn len(&self) -> usize {
        self.events.lock().expect("audit lock poisoned").len()
    }

    /// Whether no events have been recorded.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[async_trait]
impl AuditSink for InMemoryAuditSink {
    async fn record(&self, event: AuditEvent) -> Result<(), PlatformError> {
        self.events.lock().expect("audit lock poisoned").push(event);
        Ok(())
    }
}

/// An [`AuditSink`] that writes audit events to the SQL outbox via
/// [`klauthed_data::outbox::sql::SqlOutbox`].
///
/// Events are serialised as JSON into `OutboxEntry::payload`, with
/// `aggregate_type = "audit"` and `event_type = audit_event.action()`.
/// A relay process (or the existing outbox poller) delivers them from there.
///
/// Wrap in an `Arc` to share across async tasks.
#[cfg(feature = "audit-outbox")]
pub struct OutboxAuditSink {
    outbox: klauthed_data::outbox::sql::SqlOutbox,
}

#[cfg(feature = "audit-outbox")]
impl OutboxAuditSink {
    /// Wrap an existing [`SqlOutbox`](klauthed_data::outbox::sql::SqlOutbox).
    pub fn new(outbox: klauthed_data::outbox::sql::SqlOutbox) -> Self {
        Self { outbox }
    }
}

#[cfg(feature = "audit-outbox")]
#[async_trait]
impl AuditSink for OutboxAuditSink {
    async fn record(&self, event: AuditEvent) -> Result<(), PlatformError> {
        use klauthed_data::outbox::{Outbox, OutboxEntry, OutboxId};

        let payload = serde_json::to_value(&event).map_err(|e| PlatformError::Backend {
            message: format!("serialize audit event: {e}"),
        })?;

        let entry = OutboxEntry {
            id: OutboxId::new(),
            aggregate_type: "audit".to_owned(),
            aggregate_id: event.actor().unwrap_or("system").to_owned(),
            event_type: event.action().to_owned(),
            sequence: 0,
            payload,
            occurred_at: event.occurred_at(),
            published: false,
            published_at: None,
        };

        self.outbox
            .enqueue(vec![entry])
            .await
            .map_err(|e| PlatformError::Backend { message: format!("outbox enqueue: {e}") })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_defaults_and_overrides() {
        let e = AuditEvent::builder("login")
            .actor("user-1")
            .tenant("acme")
            .resource("session", "s-9")
            .metadata("ip", "127.0.0.1")
            .build();

        assert_eq!(e.action(), "login");
        assert_eq!(e.actor(), Some("user-1"));
        assert_eq!(e.tenant(), Some("acme"));
        assert_eq!(e.resource_type(), Some("session"));
        assert_eq!(e.resource_id(), Some("s-9"));
        assert_eq!(e.outcome(), AuditOutcome::Success);
        assert_eq!(e.metadata().get("ip").map(String::as_str), Some("127.0.0.1"));
    }

    #[test]
    fn failed_sets_failure_outcome() {
        let e = AuditEvent::builder("delete").failed().build();
        assert_eq!(e.outcome(), AuditOutcome::Failure);
        assert!(!e.outcome().is_success());
    }

    #[test]
    fn from_context_fills_actor_tenant_and_request_id() {
        let ctx = RequestContext::new().with_principal("p-7").with_tenant("acme");
        let e = AuditEvent::builder("read").from_context(&ctx).build();
        assert_eq!(e.actor(), Some("p-7"));
        assert_eq!(e.tenant(), Some("acme"));
        assert_eq!(
            e.metadata().get("request_id").map(String::as_str),
            Some(ctx.request_id().to_string().as_str())
        );
    }

    #[test]
    fn from_context_does_not_overwrite_explicit_values() {
        let ctx = RequestContext::new().with_principal("ctx-actor");
        let e = AuditEvent::builder("x").actor("explicit").from_context(&ctx).build();
        assert_eq!(e.actor(), Some("explicit"));
    }

    #[test]
    fn event_round_trips_through_json() {
        let e = AuditEvent::builder("x").actor("a").metadata("k", "v").build();
        let json = serde_json::to_string(&e).unwrap();
        let back: AuditEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(e, back);
    }

    #[tokio::test]
    async fn in_memory_sink_retains_events() {
        let sink = InMemoryAuditSink::new();
        assert!(sink.is_empty());

        sink.record(AuditEvent::builder("a").build()).await.unwrap();
        sink.record(AuditEvent::builder("b").build()).await.unwrap();

        let events = sink.events();
        assert_eq!(sink.len(), 2);
        assert_eq!(events[0].action(), "a");
        assert_eq!(events[1].action(), "b");
    }

    // ── OutboxAuditSink tests (require `audit-outbox` feature + SQLite) ───────

    #[cfg(feature = "audit-outbox")]
    mod outbox_sink_tests {
        use super::*;
        use klauthed_data::outbox::sql::SqlOutbox;
        use klauthed_data::outbox::{Outbox, OutboxEntry};

        async fn memory_outbox() -> SqlOutbox {
            sqlx::any::install_default_drivers();
            let pool = sqlx::pool::PoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .expect("connect in-memory sqlite");
            let outbox = SqlOutbox::new(pool);
            outbox.ensure_schema().await.expect("ensure schema");
            outbox
        }

        #[tokio::test]
        async fn record_inserts_one_entry_into_outbox() {
            let outbox = memory_outbox().await;
            let sink = OutboxAuditSink::new(outbox.clone());

            let event = AuditEvent::builder("tenant.suspend")
                .actor("admin-1")
                .tenant("acme")
                .outcome(AuditOutcome::Success)
                .build();

            sink.record(event).await.unwrap();

            let entries: Vec<OutboxEntry> = outbox.fetch_unpublished(10).await.unwrap();
            assert_eq!(entries.len(), 1);
        }

        #[tokio::test]
        async fn record_payload_round_trips_as_audit_event() {
            let outbox = memory_outbox().await;
            let sink = OutboxAuditSink::new(outbox.clone());

            let original = AuditEvent::builder("user.login")
                .actor("u-99")
                .tenant("beta")
                .metadata("ip", "10.0.0.1")
                .build();

            sink.record(original.clone()).await.unwrap();

            let entries: Vec<OutboxEntry> = outbox.fetch_unpublished(10).await.unwrap();
            assert_eq!(entries.len(), 1);

            let recovered: AuditEvent = serde_json::from_value(entries[0].payload.clone()).unwrap();
            assert_eq!(recovered, original);
        }

        #[tokio::test]
        async fn record_sets_aggregate_type_and_event_type() {
            let outbox = memory_outbox().await;
            let sink = OutboxAuditSink::new(outbox.clone());

            let event = AuditEvent::builder("invoice.paid").actor("svc-billing").build();
            let action = event.action().to_owned();

            sink.record(event).await.unwrap();

            let entries: Vec<OutboxEntry> = outbox.fetch_unpublished(10).await.unwrap();
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].aggregate_type, "audit");
            assert_eq!(entries[0].event_type, action);
        }
    }
}
