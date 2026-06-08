//! Transactional outbox.
//!
//! The outbox pattern makes "change state **and** publish an event" atomic
//! without a distributed transaction: the producer writes domain events into an
//! outbox table *in the same transaction* as the state change, and a separate
//! relay later reads unpublished entries and ships them to the broker, marking
//! them published. This crate provides the backend-agnostic [`Outbox`] trait,
//! the [`OutboxEntry`] row model, and an in-memory implementation for tests.
//!
//! A real Postgres-backed `Outbox` (selecting `FOR UPDATE SKIP LOCKED`) is a
//! future pass; the trait is shaped so that backend can drop in unchanged.
//!
//! ```
//! use klauthed_data::outbox::{InMemoryOutbox, Outbox};
//!
//! # async fn run() -> Result<(), klauthed_data::DataError> {
//! let outbox = InMemoryOutbox::new();
//! let unpublished = outbox.fetch_unpublished(10).await?;
//! assert!(unpublished.is_empty());
//! # Ok(())
//! # }
//! ```

#[cfg(feature = "sql")]
pub mod sql;

#[cfg(feature = "mongodb")]
pub mod mongo;

#[cfg(feature = "sql")]
pub use sql::SqlOutbox;

#[cfg(feature = "mongodb")]
pub use mongo::MongoOutbox;

use async_trait::async_trait;
use klauthed_core::domain::{DomainEvent, EventEnvelope};
use klauthed_core::id::Id;
use klauthed_core::time::Timestamp;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

use crate::error::DataError;

/// Marker tag for an [`OutboxEntry`]'s identity.
pub struct OutboxTag;

/// The id minted for each persisted outbox row.
pub type OutboxId = Id<OutboxTag>;

/// One row in the outbox: a serialized domain event awaiting publication.
///
/// Construct from an [`EventEnvelope`] via [`OutboxEntry::from_envelope`], which
/// carries over the aggregate metadata and serializes the payload to JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboxEntry {
    /// Unique id of this outbox row.
    pub id: OutboxId,
    /// The aggregate type the event belongs to, e.g. `account`.
    pub aggregate_type: String,
    /// The aggregate instance id, rendered as a string.
    pub aggregate_id: String,
    /// The stable, dotted event name, e.g. `account.opened`.
    pub event_type: String,
    /// The aggregate version this event produced (monotonic per aggregate).
    pub sequence: u64,
    /// The serialized event payload.
    pub payload: serde_json::Value,
    /// When the event occurred.
    pub occurred_at: Timestamp,
    /// Whether the relay has shipped this entry to the broker.
    pub published: bool,
    /// When the entry was marked published, if it has been.
    pub published_at: Option<Timestamp>,
}

impl OutboxEntry {
    /// Build an unpublished entry from an [`EventEnvelope`], serializing its
    /// payload to JSON.
    ///
    /// # Errors
    /// Returns [`DataError::Outbox`] if the payload cannot be serialized.
    pub fn from_envelope<E>(envelope: &EventEnvelope<E>) -> Result<Self, DataError>
    where
        E: Serialize + DomainEvent,
    {
        let payload = serde_json::to_value(&envelope.payload)
            .map_err(|e| DataError::Outbox(format!("failed to serialize event payload: {e}")))?;
        Ok(Self {
            id: OutboxId::new(),
            aggregate_type: envelope.aggregate_type.to_string(),
            aggregate_id: envelope.aggregate_id.clone(),
            event_type: envelope.event_type.to_string(),
            sequence: envelope.sequence,
            payload,
            occurred_at: envelope.occurred_at,
            published: false,
            published_at: None,
        })
    }
}

/// A durable buffer of domain events awaiting publication.
///
/// Implementations persist entries alongside aggregate state (ideally in the
/// same transaction) and a relay drains them with
/// [`fetch_unpublished`](Outbox::fetch_unpublished) /
/// [`mark_published`](Outbox::mark_published).
#[async_trait]
pub trait Outbox: Send + Sync {
    /// Persist a batch of entries (idempotent on `id` is the implementation's
    /// responsibility; the in-memory impl simply appends).
    async fn enqueue(&self, entries: Vec<OutboxEntry>) -> Result<(), DataError>;

    /// Return up to `limit` unpublished entries, oldest first.
    async fn fetch_unpublished(&self, limit: usize) -> Result<Vec<OutboxEntry>, DataError>;

    /// Mark the given entries published (no-op for ids that are absent or
    /// already published).
    async fn mark_published(&self, ids: &[OutboxId]) -> Result<(), DataError>;
}

/// A thread-safe, in-memory [`Outbox`] for tests and single-process use.
///
/// Not durable: entries live only for the lifetime of the process.
#[derive(Default)]
pub struct InMemoryOutbox {
    entries: Mutex<Vec<OutboxEntry>>,
}

impl InMemoryOutbox {
    /// An empty outbox.
    pub fn new() -> Self {
        Self::default()
    }

    /// The total number of entries held (published or not).
    pub fn len(&self) -> usize {
        self.entries.lock().unwrap_or_else(std::sync::PoisonError::into_inner).len()
    }

    /// Whether the outbox holds no entries at all.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[async_trait]
impl Outbox for InMemoryOutbox {
    async fn enqueue(&self, entries: Vec<OutboxEntry>) -> Result<(), DataError> {
        self.entries.lock().unwrap_or_else(std::sync::PoisonError::into_inner).extend(entries);
        Ok(())
    }

    async fn fetch_unpublished(&self, limit: usize) -> Result<Vec<OutboxEntry>, DataError> {
        let guard = self.entries.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut unpublished: Vec<OutboxEntry> =
            guard.iter().filter(|e| !e.published).cloned().collect();
        // Oldest first by id (UUID v7 is time-sortable), then truncate.
        unpublished.sort_by_key(|e| e.id);
        unpublished.truncate(limit);
        Ok(unpublished)
    }

    async fn mark_published(&self, ids: &[OutboxId]) -> Result<(), DataError> {
        let now = Timestamp::now();
        let mut guard = self.entries.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        for entry in guard.iter_mut() {
            if !entry.published && ids.contains(&entry.id) {
                entry.published = true;
                entry.published_at = Some(now);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;

    #[derive(Debug, Serialize)]
    struct Opened {
        owner: String,
    }

    impl DomainEvent for Opened {
        fn event_type(&self) -> &'static str {
            "account.opened"
        }
    }

    fn envelope(seq: u64) -> EventEnvelope<Opened> {
        EventEnvelope {
            event_id: Id::new(),
            event_type: Cow::Borrowed("account.opened"),
            aggregate_id: "acct-1".to_owned(),
            aggregate_type: Cow::Borrowed("account"),
            sequence: seq,
            occurred_at: Timestamp::from_unix_millis(1_000),
            payload: Opened { owner: "alice".to_owned() },
        }
    }

    #[test]
    fn from_envelope_carries_metadata_and_serializes_payload() {
        let entry = OutboxEntry::from_envelope(&envelope(7)).unwrap();
        assert_eq!(entry.aggregate_type, "account");
        assert_eq!(entry.aggregate_id, "acct-1");
        assert_eq!(entry.event_type, "account.opened");
        assert_eq!(entry.sequence, 7);
        assert_eq!(entry.payload["owner"], "alice");
        assert!(!entry.published);
        assert!(entry.published_at.is_none());
    }

    #[tokio::test]
    async fn enqueue_fetch_mark_published_round_trip() {
        let outbox = InMemoryOutbox::new();
        let e1 = OutboxEntry::from_envelope(&envelope(1)).unwrap();
        let e2 = OutboxEntry::from_envelope(&envelope(2)).unwrap();
        let (id1, id2) = (e1.id, e2.id);

        outbox.enqueue(vec![e1, e2]).await.unwrap();
        assert_eq!(outbox.len(), 2);

        let unpublished = outbox.fetch_unpublished(10).await.unwrap();
        assert_eq!(unpublished.len(), 2);

        // Publish only the first; it should drop out of the next fetch.
        outbox.mark_published(&[id1]).await.unwrap();
        let remaining = outbox.fetch_unpublished(10).await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, id2);

        outbox.mark_published(&[id2]).await.unwrap();
        assert!(outbox.fetch_unpublished(10).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn fetch_unpublished_honors_limit() {
        let outbox = InMemoryOutbox::new();
        let entries = (1..=5).map(|s| OutboxEntry::from_envelope(&envelope(s)).unwrap()).collect();
        outbox.enqueue(entries).await.unwrap();

        assert_eq!(outbox.fetch_unpublished(2).await.unwrap().len(), 2);
        assert_eq!(outbox.fetch_unpublished(100).await.unwrap().len(), 5);
    }
}
