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

#[cfg(test)]
mod tests {
    use super::*;

    struct Opened;
    impl DomainEvent for Opened {
        fn event_type(&self) -> &'static str {
            "account.opened"
        }
    }

    struct Migrated;
    impl DomainEvent for Migrated {
        fn event_type(&self) -> &'static str {
            "account.migrated"
        }
        fn schema_version(&self) -> u32 {
            3
        }
    }

    #[test]
    fn domain_event_schema_version_defaults_to_one_and_can_override() {
        assert_eq!(Opened.schema_version(), 1);
        assert_eq!(Migrated.schema_version(), 3);
    }

    #[test]
    fn event_log_records_takes_and_tracks_version() {
        let mut log: EventLog<&str> = EventLog::new();
        assert_eq!(log.version(), 0);
        assert!(log.is_empty());
        assert_eq!(log.len(), 0);

        log.record("e1");
        log.record("e2");
        assert_eq!(log.version(), 2);
        assert_eq!(log.len(), 2);
        assert!(!log.is_empty());
        assert_eq!(log.pending(), &["e1", "e2"]);

        // `take` drains pending events but leaves the (persistent) version.
        let taken = log.take();
        assert_eq!(taken, vec!["e1", "e2"]);
        assert!(log.is_empty());
        assert_eq!(log.version(), 2);

        // Recording continues to advance the version after a take.
        log.record("e3");
        assert_eq!(log.version(), 3);
        assert_eq!(log.pending(), &["e3"]);
    }

    #[test]
    fn event_log_with_version_and_default() {
        let log: EventLog<&str> = EventLog::with_version(7);
        assert_eq!(log.version(), 7);
        assert!(log.is_empty());

        let default: EventLog<&str> = EventLog::default();
        assert_eq!(default.version(), 0);
        assert!(default.is_empty());
    }

    #[test]
    fn event_envelope_wraps_payload_with_metadata() {
        let occurred_at = Timestamp::from_unix_seconds(1_700_000_000);
        let envelope = EventEnvelope::new("acct-1".to_owned(), "Account", 5, occurred_at, Opened);

        assert_eq!(envelope.event_type, "account.opened");
        assert_eq!(envelope.aggregate_id, "acct-1");
        assert_eq!(envelope.aggregate_type, "Account");
        assert_eq!(envelope.sequence, 5);
        assert_eq!(envelope.occurred_at, occurred_at);

        // Each occurrence mints a distinct id.
        let other = EventEnvelope::new("acct-1".to_owned(), "Account", 6, occurred_at, Opened);
        assert_ne!(envelope.event_id, other.event_id);
    }
}
