//! Domain events: [`DomainEvent`], [`EventEnvelope`], and the embedded
//! [`EventLog`] recorder.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::id::Id;
use crate::time::Timestamp;

/// Marker tag for an event identifier.
pub struct EventTag;

/// The id minted for each emitted event.
pub type EventId = Id<EventTag>;

/// A fact that happened in the domain.
pub trait DomainEvent {
    /// A stable, dotted event name, e.g. `account.opened`.
    fn event_type(&self) -> &'static str;

    /// Payload schema version, for evolving event shapes over time.
    fn schema_version(&self) -> u32 {
        1
    }
}

/// A [`DomainEvent`] wrapped with the metadata needed to transport and store it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventEnvelope<E> {
    /// Unique id of this event occurrence.
    pub event_id: EventId,
    /// The event's stable type name.
    pub event_type: Cow<'static, str>,
    /// The aggregate this event belongs to (its id rendered as a string).
    pub aggregate_id: String,
    /// The aggregate's type name.
    pub aggregate_type: Cow<'static, str>,
    /// The aggregate version this event produced (monotonic per aggregate).
    pub sequence: u64,
    /// When the event occurred.
    pub occurred_at: Timestamp,
    /// The event payload.
    pub payload: E,
}

impl<E: DomainEvent> EventEnvelope<E> {
    /// Wrap `payload` with transport metadata.
    pub fn new(
        aggregate_id: String,
        aggregate_type: &'static str,
        sequence: u64,
        occurred_at: Timestamp,
        payload: E,
    ) -> Self {
        Self {
            event_id: EventId::new(),
            event_type: Cow::Borrowed(payload.event_type()),
            aggregate_id,
            aggregate_type: Cow::Borrowed(aggregate_type),
            sequence,
            occurred_at,
            payload,
        }
    }
}

// ── Event log (embedded recorder) ─────────────────────────────────────────────

/// Tracks an aggregate's uncommitted events and its version.
///
/// Aggregates embed an `EventLog<MyEvent>` and delegate the [`AggregateRoot`](super::AggregateRoot)
/// event methods to it, so they don't re-implement the bookkeeping.
#[derive(Debug, Clone)]
pub struct EventLog<E> {
    pending: Vec<E>,
    version: u64,
}

impl<E> EventLog<E> {
    /// A fresh log at version 0 (a brand-new aggregate).
    pub fn new() -> Self {
        Self { pending: Vec::new(), version: 0 }
    }

    /// A log for an aggregate loaded at an existing `version`.
    pub fn with_version(version: u64) -> Self {
        Self { pending: Vec::new(), version }
    }

    /// Record an event and advance the version.
    pub fn record(&mut self, event: E) {
        self.pending.push(event);
        self.version += 1;
    }

    /// Drain the pending events (the version is unchanged — it is persistent).
    pub fn take(&mut self) -> Vec<E> {
        std::mem::take(&mut self.pending)
    }

    /// The pending (uncommitted) events.
    pub fn pending(&self) -> &[E] {
        &self.pending
    }

    /// The current version (number of events ever recorded).
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Whether there are no pending events.
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// The number of pending events.
    pub fn len(&self) -> usize {
        self.pending.len()
    }
}

impl<E> Default for EventLog<E> {
    fn default() -> Self {
        Self::new()
    }
}
