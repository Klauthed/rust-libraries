//! Redis-backed [`IdempotencyStore`].
//!
//! [`RedisIdempotencyStore`] implements the idempotency protocol on top of Redis:
//!
//! * **begin** — `SET key <json> NX PX <ttl_ms>`: claims the key atomically.
//!   If the key already exists, `GET` reveals whether a request is
//!   [`InProgress`](crate::idempotency::Outcome::InProgress) or
//!   [`Completed`](crate::idempotency::Outcome::Completed).
//! * **complete** — overwrites the stored record (same TTL) with status
//!   `Completed` and the caller's response payload.
//! * **get** — `GET` + deserialise.
//!
//! Keys auto-expire after the configured TTL, so the keyspace self-cleans without
//! a background job.
//!
//! # Caveats
//!
//! `complete` is a `GET`-then-`SET`: if the key expires between the two calls a
//! `DataError::Idempotency` is returned so the caller can decide how to handle
//! the edge case. This is the standard single-instance Redis trade-off; for
//! distributed atomicity a Lua compare-and-swap would be needed.
//!
//! Tests that need a live Redis are marked `#[ignore]`; run them with a server
//! at `REDIS_URL` via `cargo test -p klauthed-data --features redis -- --ignored`.

use async_trait::async_trait;
use klauthed_core::time::Timestamp;
use redis::aio::ConnectionManager;
use redis::{ExistenceCheck, SetExpiry, SetOptions};
use serde::{Deserialize, Serialize};

use crate::error::DataError;
use crate::idempotency::{IdempotencyRecord, IdempotencyStatus, IdempotencyStore, Outcome};

/// Default TTL for idempotency keys: 24 hours.
const DEFAULT_TTL_MS: u64 = 24 * 60 * 60 * 1_000;

/// A Redis-backed [`IdempotencyStore`].
///
/// Records are serialised as JSON and stored with a configurable TTL so the
/// keyspace self-cleans. Clone-cheap: holds only a [`ConnectionManager`] (an
/// `Arc` internally).
#[derive(Clone)]
pub struct RedisIdempotencyStore {
    conn: ConnectionManager,
    ttl_ms: u64,
}

impl RedisIdempotencyStore {
    /// Wrap a managed Redis connection with the default 24-hour TTL.
    pub fn new(conn: ConnectionManager) -> Self {
        Self { conn, ttl_ms: DEFAULT_TTL_MS }
    }

    /// Wrap a managed Redis connection with a custom TTL in milliseconds.
    pub fn with_ttl_ms(conn: ConnectionManager, ttl_ms: u64) -> Self {
        Self { conn, ttl_ms }
    }
}

/// The shape stored in Redis for each idempotency key.
#[derive(Serialize, Deserialize)]
struct StoredRecord {
    status: IdempotencyStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    response: Option<serde_json::Value>,
    created_at: Timestamp,
    updated_at: Timestamp,
}

#[async_trait]
impl IdempotencyStore for RedisIdempotencyStore {
    async fn begin(&self, key: &str) -> Result<Outcome, DataError> {
        let now = Timestamp::now();
        let initial = serde_json::to_string(&StoredRecord {
            status: IdempotencyStatus::InProgress,
            response: None,
            created_at: now,
            updated_at: now,
        })
        .map_err(|e| DataError::Idempotency(format!("serialisation failed: {e}")))?;

        let options = SetOptions::default()
            .conditional_set(ExistenceCheck::NX)
            .with_expiration(SetExpiry::PX(self.ttl_ms));

        let mut conn = self.conn.clone();
        // `SET … NX` returns `Some("OK")` on success, `None` when the key exists.
        let claimed: Option<String> =
            redis::cmd("SET").arg(key).arg(&initial).arg(&options).query_async(&mut conn).await?;

        if claimed.is_some() {
            return Ok(Outcome::New);
        }

        // Key already exists — inspect its current state.
        let raw: Option<String> = redis::cmd("GET").arg(key).query_async(&mut conn).await?;

        match raw {
            // Key expired between our NX attempt and GET — treat as new claim.
            None => Ok(Outcome::New),
            Some(s) => {
                let record: StoredRecord = serde_json::from_str(&s).map_err(|e| {
                    DataError::Idempotency(format!("corrupt idempotency record for '{key}': {e}"))
                })?;
                match record.status {
                    IdempotencyStatus::InProgress => Ok(Outcome::InProgress),
                    IdempotencyStatus::Completed => {
                        Ok(Outcome::Completed(record.response.unwrap_or(serde_json::Value::Null)))
                    }
                }
            }
        }
    }

    async fn complete(&self, key: &str, response: serde_json::Value) -> Result<(), DataError> {
        let now = Timestamp::now();
        let mut conn = self.conn.clone();

        // Read the current record to preserve `created_at`.
        let raw: Option<String> = redis::cmd("GET").arg(key).query_async(&mut conn).await?;

        let created_at = match raw {
            None => {
                return Err(DataError::Idempotency(format!(
                    "cannot complete unknown idempotency key '{key}'"
                )));
            }
            Some(s) => {
                serde_json::from_str::<StoredRecord>(&s).map(|r| r.created_at).unwrap_or(now)
            }
        };

        let completed = serde_json::to_string(&StoredRecord {
            status: IdempotencyStatus::Completed,
            response: Some(response),
            created_at,
            updated_at: now,
        })
        .map_err(|e| DataError::Idempotency(format!("serialisation failed: {e}")))?;

        // Overwrite with the same TTL (key existed a moment ago; if it expired
        // in the gap the SET recreates it as Completed, which is correct).
        redis::cmd("SET")
            .arg(key)
            .arg(&completed)
            .arg("PX")
            .arg(self.ttl_ms)
            .query_async::<()>(&mut conn)
            .await?;

        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<IdempotencyRecord>, DataError> {
        let mut conn = self.conn.clone();
        let raw: Option<String> = redis::cmd("GET").arg(key).query_async(&mut conn).await?;

        match raw {
            None => Ok(None),
            Some(s) => {
                let record: StoredRecord = serde_json::from_str(&s).map_err(|e| {
                    DataError::Idempotency(format!("corrupt idempotency record for '{key}': {e}"))
                })?;
                Ok(Some(IdempotencyRecord {
                    key: key.to_owned(),
                    status: record.status,
                    response: record.response,
                    created_at: record.created_at,
                    updated_at: record.updated_at,
                }))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locks::LockToken;

    /// Connect to a live Redis at `REDIS_URL` (default `redis://127.0.0.1/`).
    async fn live_store() -> RedisIdempotencyStore {
        let url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1/".to_owned());
        let client = redis::Client::open(url).expect("open redis client");
        let conn = ConnectionManager::new(client).await.expect("connect redis");
        RedisIdempotencyStore::with_ttl_ms(conn, 60_000) // 1-minute TTL for tests
    }

    #[tokio::test]
    #[ignore = "requires a live Redis at REDIS_URL"]
    async fn new_in_progress_complete_replay() {
        let store = live_store().await;
        let key = format!("klauthed:test:idem:{}", LockToken::new());

        assert_eq!(store.begin(&key).await.unwrap(), Outcome::New);
        assert_eq!(store.begin(&key).await.unwrap(), Outcome::InProgress);

        let response = serde_json::json!({"charged": true, "amount": 100});
        store.complete(&key, response.clone()).await.unwrap();

        assert_eq!(store.begin(&key).await.unwrap(), Outcome::Completed(response));
    }

    #[tokio::test]
    #[ignore = "requires a live Redis at REDIS_URL"]
    async fn complete_unknown_key_errors() {
        let store = live_store().await;
        let key = format!("klauthed:test:idem:{}:missing", LockToken::new());

        let err = store.complete(&key, serde_json::Value::Null).await.unwrap_err();
        assert!(matches!(err, DataError::Idempotency(_)));
    }

    #[tokio::test]
    #[ignore = "requires a live Redis at REDIS_URL"]
    async fn get_returns_record_lifecycle() {
        let store = live_store().await;
        let key = format!("klauthed:test:idem:{}", LockToken::new());

        assert!(store.get(&key).await.unwrap().is_none());

        store.begin(&key).await.unwrap();
        let rec = store.get(&key).await.unwrap().unwrap();
        assert_eq!(rec.status, IdempotencyStatus::InProgress);
        assert!(rec.response.is_none());

        store.complete(&key, serde_json::json!(42)).await.unwrap();
        let rec = store.get(&key).await.unwrap().unwrap();
        assert_eq!(rec.status, IdempotencyStatus::Completed);
        assert_eq!(rec.response, Some(serde_json::json!(42)));
    }

    #[tokio::test]
    #[ignore = "requires a live Redis at REDIS_URL"]
    async fn distinct_keys_are_independent() {
        let store = live_store().await;
        let a = format!("klauthed:test:idem:{}:a", LockToken::new());
        let b = format!("klauthed:test:idem:{}:b", LockToken::new());

        assert_eq!(store.begin(&a).await.unwrap(), Outcome::New);
        assert_eq!(store.begin(&b).await.unwrap(), Outcome::New);
        assert_eq!(store.begin(&a).await.unwrap(), Outcome::InProgress);
    }
}
