//! Draining the [`Outbox`]: an [`OutboxPublisher`] sink and an [`OutboxRelay`]
//! that ships unpublished entries to it.
//!
//! The transactional outbox stores events alongside aggregate state; a relay
//! then drains them to a broker. Call [`OutboxRelay::drain`] periodically (e.g.
//! from a background task or `klauthed-platform`'s scheduler):
//!
//! ```no_run
//! # async fn run(
//! #     outbox: &dyn klauthed_data::Outbox,
//! #     publisher: &dyn klauthed_data::OutboxPublisher,
//! # ) -> Result<(), klauthed_data::DataError> {
//! use klauthed_data::OutboxRelay;
//!
//! let relay = OutboxRelay::new();
//! let published = relay.drain(outbox, publisher).await?;
//! # let _ = published;
//! # Ok(())
//! # }
//! ```

use async_trait::async_trait;

use crate::error::DataError;

use super::{Outbox, OutboxEntry, OutboxId};

/// A sink that ships an [`OutboxEntry`] to its destination — a message broker,
/// event bus, HTTP endpoint, etc. Implement this for your transport.
#[async_trait]
pub trait OutboxPublisher: Send + Sync {
    /// Publish one entry. Returning `Err` leaves the entry (and any after it in
    /// the batch) unpublished, so the next drain retries it — at-least-once.
    async fn publish(&self, entry: &OutboxEntry) -> Result<(), DataError>;
}

/// Drains an [`Outbox`] in batches: fetch unpublished entries oldest-first,
/// publish each via an [`OutboxPublisher`], and mark the successful ones.
#[derive(Debug, Clone, Copy)]
pub struct OutboxRelay {
    batch_size: usize,
}

impl OutboxRelay {
    /// A relay draining up to 100 entries per pass.
    #[must_use]
    pub fn new() -> Self {
        Self { batch_size: 100 }
    }

    /// Set the maximum number of entries drained per [`drain`](Self::drain) pass
    /// (clamped to at least 1).
    #[must_use]
    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = batch_size.max(1);
        self
    }

    /// Run one drain pass: fetch unpublished entries, publish each in order, and
    /// mark the ones that succeeded. Returns the number published.
    ///
    /// Publishing stops at the first failure so ordering is preserved and the
    /// failed entry (plus those after it) retry on the next pass.
    ///
    /// # Errors
    /// Returns [`DataError`] if the outbox fetch or mark-published call fails. A
    /// *publish* failure is not an error — it just ends this pass early.
    pub async fn drain(
        &self,
        outbox: &(impl Outbox + ?Sized),
        publisher: &(impl OutboxPublisher + ?Sized),
    ) -> Result<usize, DataError> {
        let entries = outbox.fetch_unpublished(self.batch_size).await?;

        let mut published: Vec<OutboxId> = Vec::new();
        for entry in &entries {
            if publisher.publish(entry).await.is_err() {
                break;
            }
            published.push(entry.id);
        }

        if !published.is_empty() {
            outbox.mark_published(&published).await?;
        }
        Ok(published.len())
    }
}

impl Default for OutboxRelay {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outbox::InMemoryOutbox;
    use klauthed_core::time::Timestamp;
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingPublisher {
        published: Mutex<Vec<OutboxId>>,
        /// Start failing once this many entries have been published.
        fail_after: Option<usize>,
    }

    #[async_trait]
    impl OutboxPublisher for RecordingPublisher {
        async fn publish(&self, entry: &OutboxEntry) -> Result<(), DataError> {
            let mut published =
                self.published.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if self.fail_after.is_some_and(|n| published.len() >= n) {
                return Err(DataError::Outbox("publish failed".into()));
            }
            published.push(entry.id);
            Ok(())
        }
    }

    impl RecordingPublisher {
        fn count(&self) -> usize {
            self.published.lock().unwrap_or_else(std::sync::PoisonError::into_inner).len()
        }
    }

    fn entry() -> OutboxEntry {
        OutboxEntry {
            id: OutboxId::new(),
            aggregate_type: "account".into(),
            aggregate_id: "a1".into(),
            event_type: "account.opened".into(),
            sequence: 1,
            payload: serde_json::json!({ "ok": true }),
            occurred_at: Timestamp::now(),
            published: false,
            published_at: None,
        }
    }

    #[tokio::test]
    async fn drains_all_unpublished_entries() {
        let outbox = InMemoryOutbox::new();
        outbox.enqueue(vec![entry(), entry(), entry()]).await.unwrap();
        let publisher = RecordingPublisher::default();

        let published = OutboxRelay::new().drain(&outbox, &publisher).await.unwrap();

        assert_eq!(published, 3);
        assert_eq!(publisher.count(), 3);
        // All marked published → nothing left to drain.
        assert!(outbox.fetch_unpublished(10).await.unwrap().is_empty());
        let second = OutboxRelay::new().drain(&outbox, &publisher).await.unwrap();
        assert_eq!(second, 0);
    }

    #[tokio::test]
    async fn stops_at_first_failure_then_retries_remainder() {
        let outbox = InMemoryOutbox::new();
        outbox.enqueue(vec![entry(), entry(), entry()]).await.unwrap();

        // Fails on the 2nd publish: only the 1st is marked.
        let failing = RecordingPublisher { fail_after: Some(1), ..Default::default() };
        let first = OutboxRelay::new().drain(&outbox, &failing).await.unwrap();
        assert_eq!(first, 1);
        assert_eq!(outbox.fetch_unpublished(10).await.unwrap().len(), 2, "two still pending");

        // A healthy publisher drains the rest on the next pass.
        let healthy = RecordingPublisher::default();
        let second = OutboxRelay::new().drain(&outbox, &healthy).await.unwrap();
        assert_eq!(second, 2);
        assert!(outbox.fetch_unpublished(10).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn empty_outbox_drains_nothing() {
        let outbox = InMemoryOutbox::new();
        let published =
            OutboxRelay::new().drain(&outbox, &RecordingPublisher::default()).await.unwrap();
        assert_eq!(published, 0);
    }

    #[tokio::test]
    async fn batch_size_limits_a_pass() {
        let outbox = InMemoryOutbox::new();
        outbox.enqueue(vec![entry(), entry(), entry()]).await.unwrap();
        let publisher = RecordingPublisher::default();

        let relay = OutboxRelay::new().with_batch_size(2);
        assert_eq!(relay.drain(&outbox, &publisher).await.unwrap(), 2);
        assert_eq!(relay.drain(&outbox, &publisher).await.unwrap(), 1);
    }
}
