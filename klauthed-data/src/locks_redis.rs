//! Redis-backed [`LockManager`].
//!
//! [`RedisLockManager`] implements the standard single-instance Redis lock:
//!
//! * **acquire** — `SET key token NX PX <ttl_ms>`. `NX` makes the write succeed
//!   only when the key is free; `PX` bounds the hold so a crashed owner cannot
//!   wedge the lock forever. The random `token` is the fencing token returned in
//!   the [`LockGuard`].
//! * **release** — a Lua compare-and-delete that removes the key only if its
//!   value still equals our token, so a guard that outlived its TTL (and was
//!   re-acquired by someone else) cannot delete the new owner's lock.
//!
//! This is the classic single-node algorithm; it is *not* Redlock and offers no
//! guarantees across a failed-over Redis. For most service coordination needs
//! (leader election, throttling a cron) it is sufficient.
//!
//! Tests that need a live Redis are marked `#[ignore]`; run them with a server
//! at `REDIS_URL` via `cargo test -p klauthed-data --features redis -- --ignored`.

use async_trait::async_trait;
use chrono::Duration;
use redis::aio::ConnectionManager;
use redis::{ExistenceCheck, SetExpiry, SetOptions};

use crate::error::DataError;
use crate::locks::{LockGuard, LockManager, LockToken};

/// Lua script: delete `KEYS[1]` only if its value equals `ARGV[1]`.
/// Returns `1` if it deleted, `0` otherwise.
const RELEASE_SCRIPT: &str = "\
if redis.call('GET', KEYS[1]) == ARGV[1] then
    return redis.call('DEL', KEYS[1])
else
    return 0
end";

/// A [`LockManager`] that grants TTL-bounded locks via Redis `SET … NX PX`.
///
/// Clone-cheap: holds a cloneable [`ConnectionManager`].
#[derive(Clone)]
pub struct RedisLockManager {
    conn: ConnectionManager,
}

impl RedisLockManager {
    /// Wrap a managed Redis connection (see `cache::connect_redis`).
    pub fn new(conn: ConnectionManager) -> Self {
        Self { conn }
    }

    /// Release a key by fencing `token`, honoring the compare-and-delete so only
    /// the still-current holder is freed. Returns `true` if the lock was held by
    /// `token` and is now released, `false` if it had already expired or been
    /// taken over.
    ///
    /// Exposed so a process can release a lock by token without holding the
    /// [`LockGuard`] (e.g. after recovering the token from elsewhere). The guard
    /// returned by [`acquire`](LockManager::acquire) releases through this on
    /// [`release`](LockGuard::release) / drop.
    pub async fn release_token(&self, key: &str, token: LockToken) -> Result<bool, DataError> {
        let mut conn = self.conn.clone();
        let deleted: i64 = redis::Script::new(RELEASE_SCRIPT)
            .key(key)
            .arg(token.to_string())
            .invoke_async(&mut conn)
            .await?;
        Ok(deleted == 1)
    }
}

#[async_trait]
impl LockManager for RedisLockManager {
    async fn acquire(&self, key: &str, ttl: Duration) -> Result<Option<LockGuard>, DataError> {
        let ttl_ms: u64 = ttl
            .num_milliseconds()
            .try_into()
            .map_err(|_| DataError::LockHeld(format!("invalid (non-positive) TTL for lock '{key}'")))?;
        if ttl_ms == 0 {
            return Err(DataError::LockHeld(format!(
                "invalid (zero) TTL for lock '{key}'"
            )));
        }

        let token = LockToken::new();
        let options = SetOptions::default()
            .conditional_set(ExistenceCheck::NX)
            .with_expiration(SetExpiry::PX(ttl_ms));

        let mut conn = self.conn.clone();
        // `SET … NX` returns the value on success and nil (None) when the key
        // already exists, so a `None` means we lost the race.
        let outcome: Option<String> = redis::cmd("SET")
            .arg(key)
            .arg(token.to_string())
            .arg(&options)
            .query_async(&mut conn)
            .await?;

        match outcome {
            Some(_) => Ok(Some(LockGuard::redis(
                key.to_owned(),
                token,
                self.clone(),
            ))),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a manager against a live Redis from `REDIS_URL` (default
    /// `redis://127.0.0.1/`). Used only by ignored tests.
    async fn live_manager() -> RedisLockManager {
        let url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1/".to_owned());
        let client = redis::Client::open(url).expect("open redis client");
        let conn = ConnectionManager::new(client)
            .await
            .expect("connect redis");
        RedisLockManager::new(conn)
    }

    #[tokio::test]
    #[ignore = "requires a live Redis at REDIS_URL"]
    async fn acquire_blocks_until_released() {
        let locks = live_manager().await;
        let key = format!("klauthed:test:lock:{}", LockToken::new());

        let guard = locks
            .acquire(&key, Duration::seconds(30))
            .await
            .unwrap()
            .expect("first acquire wins");

        // Second acquire while held returns None.
        assert!(locks
            .acquire(&key, Duration::seconds(30))
            .await
            .unwrap()
            .is_none());

        guard.release().await.unwrap();

        // Now it is free again.
        assert!(locks
            .acquire(&key, Duration::seconds(30))
            .await
            .unwrap()
            .is_some());
    }

    #[tokio::test]
    #[ignore = "requires a live Redis at REDIS_URL"]
    async fn stale_token_release_does_not_steal() {
        let locks = live_manager().await;
        let key = format!("klauthed:test:lock:{}", LockToken::new());

        let stale = locks
            .acquire(&key, Duration::milliseconds(50))
            .await
            .unwrap()
            .unwrap();
        let stale_token = stale.token();
        // Let the TTL lapse so a new holder can take the key.
        tokio::time::sleep(std::time::Duration::from_millis(120)).await;

        let _fresh = locks
            .acquire(&key, Duration::seconds(30))
            .await
            .unwrap()
            .expect("fresh acquire after expiry");

        // Releasing the stale token must NOT free the fresh holder's lock.
        let freed = locks.release_token(&key, stale_token).await.unwrap();
        assert!(!freed);
        std::mem::forget(stale);
    }
}
