//! Audit sinks: the [`AuditSink`] trait, the in-memory [`InMemoryAuditSink`],
//! and the feature-gated `OutboxAuditSink`.

use std::sync::Mutex;

use async_trait::async_trait;

use crate::error::PlatformError;

use super::AuditEvent;

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
