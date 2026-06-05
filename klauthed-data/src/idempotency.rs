//! Idempotency keys.
//!
//! To make a non-idempotent operation safe to retry, the caller attaches an
//! **idempotency key**. Before doing the work it calls [`begin`](IdempotencyStore::begin):
//!
//! * [`Outcome::New`] — no record yet; this caller claimed the key and should
//!   proceed, then call [`complete`](IdempotencyStore::complete) with the response.
//! * [`Outcome::InProgress`] — another attempt claimed the key and has not
//!   finished; the caller should back off / reject the duplicate.
//! * [`Outcome::Completed`] — the work already ran; the stored response is
//!   replayed instead of re-executing.
//!
//! This module provides the backend-agnostic [`IdempotencyStore`] trait, the
//! [`IdempotencyRecord`] model, and an in-memory implementation. A Redis-backed
//! store (using `SET key val NX` to claim atomically) is a future pass.
//!
//! ```
//! use klauthed_data::idempotency::{IdempotencyStore, InMemoryIdempotencyStore, Outcome};
//!
//! # async fn run() -> Result<(), klauthed_data::DataError> {
//! let store = InMemoryIdempotencyStore::new();
//! match store.begin("charge-42").await? {
//!     Outcome::New => {
//!         store.complete("charge-42", serde_json::json!({"ok": true})).await?;
//!     }
//!     Outcome::InProgress => { /* duplicate in flight */ }
//!     Outcome::Completed(_response) => { /* replay stored response */ }
//! }
//! # Ok(())
//! # }
//! ```

use async_trait::async_trait;
use klauthed_core::time::Timestamp;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

use crate::error::DataError;

/// The lifecycle state of an idempotency key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IdempotencyStatus {
    /// Claimed by a caller that is still executing the operation.
    InProgress,
    /// The operation finished; a response is stored for replay.
    Completed,
}

/// The persisted state for one idempotency key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdempotencyRecord {
    /// The idempotency key this record belongs to.
    pub key: String,
    /// Where the keyed operation is in its lifecycle.
    pub status: IdempotencyStatus,
    /// The stored response, present once `status` is
    /// [`Completed`](IdempotencyStatus::Completed).
    pub response: Option<serde_json::Value>,
    /// When the key was first claimed.
    pub created_at: Timestamp,
    /// When the record last changed (claim or completion).
    pub updated_at: Timestamp,
}

/// The result of claiming an idempotency key with [`IdempotencyStore::begin`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The key was free and is now claimed by this caller; proceed with the work.
    New,
    /// The key is claimed by an attempt that has not yet completed.
    InProgress,
    /// The work already completed; the stored response is returned for replay.
    Completed(serde_json::Value),
}

/// A store that deduplicates operations by idempotency key.
#[async_trait]
pub trait IdempotencyStore: Send + Sync {
    /// Atomically claim `key` if it is free, otherwise report its current state.
    ///
    /// Returns [`Outcome::New`] when this caller wins the claim (a record is
    /// created `InProgress`), [`Outcome::InProgress`] if another claim is live,
    /// or [`Outcome::Completed`] with the stored response if the work is done.
    async fn begin(&self, key: &str) -> Result<Outcome, DataError>;

    /// Mark `key`'s operation completed and store `response` for future replays.
    ///
    /// # Errors
    /// Returns [`DataError::Idempotency`] if the key was never claimed.
    async fn complete(&self, key: &str, response: serde_json::Value) -> Result<(), DataError>;

    /// Fetch the raw record for `key`, if any.
    async fn get(&self, key: &str) -> Result<Option<IdempotencyRecord>, DataError>;
}

/// A thread-safe, in-memory [`IdempotencyStore`] for tests and single-process use.
#[derive(Default)]
pub struct InMemoryIdempotencyStore {
    records: Mutex<HashMap<String, IdempotencyRecord>>,
}

impl InMemoryIdempotencyStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl IdempotencyStore for InMemoryIdempotencyStore {
    async fn begin(&self, key: &str) -> Result<Outcome, DataError> {
        let now = Timestamp::now();
        let mut guard = self.records.lock().expect("idempotency mutex poisoned");
        match guard.get(key) {
            Some(record) => match record.status {
                IdempotencyStatus::InProgress => Ok(Outcome::InProgress),
                IdempotencyStatus::Completed => {
                    // A completed record always carries its response.
                    let response = record.response.clone().unwrap_or(serde_json::Value::Null);
                    Ok(Outcome::Completed(response))
                }
            },
            None => {
                guard.insert(
                    key.to_owned(),
                    IdempotencyRecord {
                        key: key.to_owned(),
                        status: IdempotencyStatus::InProgress,
                        response: None,
                        created_at: now,
                        updated_at: now,
                    },
                );
                Ok(Outcome::New)
            }
        }
    }

    async fn complete(&self, key: &str, response: serde_json::Value) -> Result<(), DataError> {
        let now = Timestamp::now();
        let mut guard = self.records.lock().expect("idempotency mutex poisoned");
        match guard.get_mut(key) {
            Some(record) => {
                record.status = IdempotencyStatus::Completed;
                record.response = Some(response);
                record.updated_at = now;
                Ok(())
            }
            None => Err(DataError::Idempotency(format!(
                "cannot complete unknown idempotency key '{key}'"
            ))),
        }
    }

    async fn get(&self, key: &str) -> Result<Option<IdempotencyRecord>, DataError> {
        Ok(self
            .records
            .lock()
            .expect("idempotency mutex poisoned")
            .get(key)
            .cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn new_then_in_progress_then_completed_replay() {
        let store = InMemoryIdempotencyStore::new();

        // First caller claims the key.
        assert_eq!(store.begin("k").await.unwrap(), Outcome::New);

        // A concurrent second caller sees it in progress.
        assert_eq!(store.begin("k").await.unwrap(), Outcome::InProgress);

        // The first caller finishes and stores its response.
        let response = serde_json::json!({ "charged": true, "amount": 100 });
        store.complete("k", response.clone()).await.unwrap();

        // Subsequent begins replay the stored response instead of re-running.
        assert_eq!(store.begin("k").await.unwrap(), Outcome::Completed(response));
    }

    #[tokio::test]
    async fn distinct_keys_are_independent() {
        let store = InMemoryIdempotencyStore::new();
        assert_eq!(store.begin("a").await.unwrap(), Outcome::New);
        assert_eq!(store.begin("b").await.unwrap(), Outcome::New);
    }

    #[tokio::test]
    async fn complete_unknown_key_errors() {
        let store = InMemoryIdempotencyStore::new();
        let err = store
            .complete("missing", serde_json::Value::Null)
            .await
            .unwrap_err();
        assert!(matches!(err, DataError::Idempotency(_)));
    }

    #[tokio::test]
    async fn get_exposes_record_lifecycle() {
        let store = InMemoryIdempotencyStore::new();
        store.begin("k").await.unwrap();
        let rec = store.get("k").await.unwrap().unwrap();
        assert_eq!(rec.status, IdempotencyStatus::InProgress);
        assert!(rec.response.is_none());

        store.complete("k", serde_json::json!(1)).await.unwrap();
        let rec = store.get("k").await.unwrap().unwrap();
        assert_eq!(rec.status, IdempotencyStatus::Completed);
        assert_eq!(rec.response, Some(serde_json::json!(1)));
    }
}
