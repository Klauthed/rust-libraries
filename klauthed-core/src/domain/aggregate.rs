//! The [`AggregateRoot`] consistency boundary and its [`Repository`].

use std::fmt;

use crate::time::Timestamp;

use super::{DomainEvent, Entity, EventEnvelope, EventLog};

/// A consistency boundary that owns its invariants and records the events its
/// state changes produce.
///
/// Implementors supply [`aggregate_type`](AggregateRoot::aggregate_type) and the
/// two `event_log` accessors; everything else is provided.
pub trait AggregateRoot: Entity {
    /// The event type this aggregate emits.
    type Event: DomainEvent;

    /// A stable type name, e.g. `account`.
    fn aggregate_type() -> &'static str;

    /// The embedded event log (read).
    fn event_log(&self) -> &EventLog<Self::Event>;

    /// The embedded event log (mutate).
    fn event_log_mut(&mut self) -> &mut EventLog<Self::Event>;

    /// The current version (an optimistic-lock token).
    fn version(&self) -> u64 {
        self.event_log().version()
    }

    /// Record a domain event the aggregate just produced.
    fn record(&mut self, event: Self::Event) {
        self.event_log_mut().record(event);
    }

    /// The uncommitted events, without draining them.
    fn pending_events(&self) -> &[Self::Event] {
        self.event_log().pending()
    }

    /// Drain the uncommitted events.
    fn take_events(&mut self) -> Vec<Self::Event> {
        self.event_log_mut().take()
    }

    /// Drain the uncommitted events as [`EventEnvelope`]s ready to publish,
    /// stamping each with its aggregate id/type, sequence, and `occurred_at`.
    fn drain_envelopes(&mut self, occurred_at: Timestamp) -> Vec<EventEnvelope<Self::Event>>
    where
        Self::Id: fmt::Display,
    {
        let aggregate_id = self.id().to_string();
        let aggregate_type = Self::aggregate_type();
        let events = self.take_events();
        // Sequences for the drained events end at the current version.
        let end = self.version();
        let start = end + 1 - events.len() as u64;
        events
            .into_iter()
            .enumerate()
            .map(|(offset, payload)| {
                EventEnvelope::new(
                    aggregate_id.clone(),
                    aggregate_type,
                    start + offset as u64,
                    occurred_at,
                    payload,
                )
            })
            .collect()
    }
}

// ── Repository ────────────────────────────────────────────────────────────────

/// Persists and retrieves aggregates by identity.
///
/// The data layer (e.g. `klauthed-data`) provides concrete implementations; the
/// domain only depends on this abstraction.
#[async_trait::async_trait]
pub trait Repository<A>: Send + Sync
where
    A: AggregateRoot + Send + Sync,
    A::Id: Send + Sync,
{
    /// The error type the implementation reports.
    type Error;

    /// Load an aggregate by id, or `None` if it does not exist.
    async fn find(&self, id: &A::Id) -> Result<Option<A>, Self::Error>;

    /// Persist an aggregate. Takes `&mut` so the implementation may drain its
    /// events (e.g. into an outbox) as part of the save.
    async fn save(&self, aggregate: &mut A) -> Result<(), Self::Error>;

    /// Delete an aggregate by id.
    async fn delete(&self, id: &A::Id) -> Result<(), Self::Error>;
}
