//! Distributed locks.
//!
//! A [`LockManager`] grants mutual exclusion on a string key with a time-to-live
//! (TTL), so a crashed holder cannot wedge the lock forever — it expires.
//! [`acquire`](LockManager::acquire) returns `Some(guard)` to the winner and
//! `None` when the key is already held by a live (unexpired) lock. The
//! [`LockGuard`] releases the lock when dropped, or eagerly via
//! [`release`](LockGuard::release).
//!
//! Expiry is driven by an injected [`Clock`], so tests can pin and advance time
//! with a `FixedClock` instead of sleeping.
//!
//! [`Clock`]: klauthed_core::time::Clock
//!
//! This module provides the trait, the [`LockGuard`] model, and an in-memory
//! implementation. A Redis-backed manager (`SET key token NX PX ttl`, released
//! with a compare-and-delete Lua script) is a future pass.
//!
//! ```
//! use std::sync::Arc;
//! use chrono::Duration;
//! use klauthed_core::time::SystemClock;
//! use klauthed_data::locks::{InMemoryLockManager, LockManager};
//!
//! # async fn run() -> Result<(), klauthed_data::DataError> {
//! let locks = InMemoryLockManager::new(Arc::new(SystemClock));
//! if let Some(guard) = locks.acquire("job:nightly", Duration::seconds(30)).await? {
//!     // critical section …
//!     guard.release().await?;
//! }
//! # Ok(())
//! # }
//! ```

#[cfg(feature = "redis")]
pub mod redis;

#[cfg(feature = "mongodb")]
pub mod mongo;

use async_trait::async_trait;
use chrono::Duration;
use klauthed_core::id::Id;
use klauthed_core::time::{Clock, SystemClock, Timestamp};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::error::DataError;

/// Marker tag for a lock's fencing token.
pub struct LockTokenTag;

/// A unique token identifying a single acquisition of a lock. Used as a fencing
/// token so a release only frees the acquisition that created it.
pub type LockToken = Id<LockTokenTag>;

/// Shared lock state: key -> (token of holder, expiry instant).
type LockTable = Mutex<HashMap<String, (LockToken, Timestamp)>>;

/// A manager that grants mutually-exclusive, TTL-bounded locks by key.
#[async_trait]
pub trait LockManager: Send + Sync {
    /// Try to acquire `key` for `ttl`. Returns `Some(guard)` if the lock was
    /// free (or held by an expired acquisition), or `None` if a live holder
    /// owns it.
    ///
    /// # Errors
    /// Returns a [`DataError`] only on backend failure; contention is reported
    /// as `Ok(None)`, not an error.
    async fn acquire(
        &self,
        key: &str,
        ttl: Duration,
    ) -> Result<Option<LockGuard>, DataError>;
}

/// Which backend a [`LockGuard`] releases against.
enum LockBackend {
    /// In-memory table shared with an [`InMemoryLockManager`].
    InMemory(Arc<LockTable>),
    /// Redis-backed manager; release runs a compare-and-delete Lua script.
    #[cfg(feature = "redis")]
    Redis(self::redis::RedisLockManager),
    /// MongoDB-backed manager using compare-and-upsert with TTL.
    #[cfg(feature = "mongodb")]
    Mongo(self::mongo::MongoLockManager),
}

/// A held lock. Dropping it releases the lock; [`release`](LockGuard::release)
/// does so eagerly and lets the caller observe errors.
///
/// The guard carries a fencing [`token`](LockGuard::token): release only frees
/// the lock if this same token still owns the key, so a guard that outlived its
/// TTL (and was re-acquired by someone else) cannot stomp the new holder.
///
/// For the in-memory backend, dropping the guard releases synchronously. For the
/// Redis backend, releasing requires an async round-trip, so it happens only via
/// [`release`](LockGuard::release); a dropped-but-not-released Redis guard is
/// cleaned up by the lock's TTL instead.
pub struct LockGuard {
    key: String,
    token: LockToken,
    backend: LockBackend,
    released: bool,
}

impl LockGuard {
    /// Construct an in-memory guard (used by [`InMemoryLockManager`]).
    fn in_memory(key: String, token: LockToken, table: Arc<LockTable>) -> Self {
        Self {
            key,
            token,
            backend: LockBackend::InMemory(table),
            released: false,
        }
    }

    /// Construct a Redis-backed guard (used by `RedisLockManager`).
    #[cfg(feature = "redis")]
    pub(crate) fn redis(
        key: String,
        token: LockToken,
        manager: self::redis::RedisLockManager,
    ) -> Self {
        Self {
            key,
            token,
            backend: LockBackend::Redis(manager),
            released: false,
        }
    }

    /// Construct a MongoDB-backed guard (used by `MongoLockManager`).
    #[cfg(feature = "mongodb")]
    pub(crate) fn mongo(
        key: String,
        token: LockToken,
        manager: self::mongo::MongoLockManager,
    ) -> Self {
        Self {
            key,
            token,
            backend: LockBackend::Mongo(manager),
            released: false,
        }
    }

    /// The key this guard holds.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// The fencing token for this acquisition.
    pub fn token(&self) -> LockToken {
        self.token
    }

    /// Release the lock now (idempotent). Only frees the key if this guard's
    /// token still owns it.
    ///
    /// # Errors
    /// Returns a [`DataError`] only if a backend round-trip fails (Redis/MongoDB);
    /// the in-memory backend never errors.
    pub async fn release(mut self) -> Result<(), DataError> {
        if self.released {
            return Ok(());
        }
        self.released = true;
        match &self.backend {
            LockBackend::InMemory(table) => {
                Self::release_in_memory(table, &self.key, self.token);
                Ok(())
            }
            #[cfg(feature = "redis")]
            LockBackend::Redis(manager) => {
                manager.release_token(&self.key, self.token).await?;
                Ok(())
            }
            #[cfg(feature = "mongodb")]
            LockBackend::Mongo(manager) => {
                manager.release_token(&self.key, self.token).await?;
                Ok(())
            }
        }
    }

    /// Synchronous compare-and-delete against the in-memory table.
    fn release_in_memory(table: &LockTable, key: &str, token: LockToken) {
        let mut guard = table.lock().expect("lock table mutex poisoned");
        // Compare-and-delete: only remove if we still own the key.
        if let Some((holder, _)) = guard.get(key)
            && *holder == token
        {
            guard.remove(key);
        }
    }
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        if self.released {
            return;
        }
        self.released = true;
        match &self.backend {
            LockBackend::InMemory(table) => {
                Self::release_in_memory(table, &self.key, self.token);
            }
            // Redis/MongoDB guards can't make async calls from Drop; the lock's TTL
            // reclaims it. Callers who need prompt release should call
            // `release().await` explicitly.
            #[cfg(feature = "redis")]
            LockBackend::Redis(_) => {
                tracing::debug!(
                    key = %self.key,
                    "redis lock guard dropped without explicit release; relying on TTL expiry"
                );
            }
            #[cfg(feature = "mongodb")]
            LockBackend::Mongo(_) => {
                tracing::debug!(
                    key = %self.key,
                    "mongodb lock guard dropped without explicit release; relying on TTL expiry"
                );
            }
        }
    }
}

/// A thread-safe, in-memory [`LockManager`] for tests and single-process use.
///
/// Expiry is evaluated against an injected [`Clock`], so a `FixedClock` makes
/// TTL behavior deterministic in tests.
pub struct InMemoryLockManager {
    table: Arc<LockTable>,
    clock: Arc<dyn Clock>,
}

impl InMemoryLockManager {
    /// A manager driven by `clock` (use a `FixedClock` in tests).
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            table: Arc::new(Mutex::new(HashMap::new())),
            clock,
        }
    }
}

impl Default for InMemoryLockManager {
    /// A manager backed by the real [`SystemClock`].
    fn default() -> Self {
        Self::new(Arc::new(SystemClock))
    }
}

#[async_trait]
impl LockManager for InMemoryLockManager {
    async fn acquire(
        &self,
        key: &str,
        ttl: Duration,
    ) -> Result<Option<LockGuard>, DataError> {
        let now = self.clock.now();
        let expires_at = now
            .checked_add(ttl)
            .ok_or_else(|| DataError::LockHeld(format!("invalid TTL for lock '{key}'")))?;

        let mut guard = self.table.lock().expect("lock table mutex poisoned");

        // A key is takeable if absent or if its current holder has expired.
        let live_holder = guard
            .get(key)
            .is_some_and(|(_, holder_expiry)| now < *holder_expiry);
        if live_holder {
            return Ok(None);
        }

        let token = LockToken::new();
        guard.insert(key.to_owned(), (token, expires_at));
        drop(guard);

        Ok(Some(LockGuard::in_memory(
            key.to_owned(),
            token,
            Arc::clone(&self.table),
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use klauthed_core::time::FixedClock;

    fn manager_with(clock: Arc<FixedClock>) -> InMemoryLockManager {
        InMemoryLockManager::new(clock)
    }

    #[tokio::test]
    async fn second_acquire_is_blocked_while_held() {
        let clock = Arc::new(FixedClock::at_unix_millis(0));
        let locks = manager_with(clock);

        let first = locks
            .acquire("k", Duration::seconds(30))
            .await
            .unwrap()
            .expect("first acquire wins");
        assert_eq!(first.key(), "k");

        // Second acquire while the first is alive returns None.
        assert!(locks.acquire("k", Duration::seconds(30)).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn lock_releases_on_drop() {
        let clock = Arc::new(FixedClock::at_unix_millis(0));
        let locks = manager_with(clock);

        {
            let _guard = locks.acquire("k", Duration::seconds(30)).await.unwrap().unwrap();
            assert!(locks.acquire("k", Duration::seconds(30)).await.unwrap().is_none());
        } // guard dropped here -> released

        assert!(locks.acquire("k", Duration::seconds(30)).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn explicit_release_frees_the_lock() {
        let clock = Arc::new(FixedClock::at_unix_millis(0));
        let locks = manager_with(clock);

        let guard = locks.acquire("k", Duration::seconds(30)).await.unwrap().unwrap();
        guard.release().await.unwrap();

        assert!(locks.acquire("k", Duration::seconds(30)).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn lock_expires_after_ttl() {
        let clock = Arc::new(FixedClock::at_unix_millis(0));
        let locks = manager_with(Arc::clone(&clock));

        // Hold for 10s, then leak the guard so only TTL can free it.
        let guard = locks.acquire("k", Duration::seconds(10)).await.unwrap().unwrap();
        std::mem::forget(guard);

        // Still within TTL -> blocked.
        clock.advance(Duration::seconds(5));
        assert!(locks.acquire("k", Duration::seconds(10)).await.unwrap().is_none());

        // Past TTL -> the stale lock is considered expired and reusable.
        clock.advance(Duration::seconds(6));
        assert!(locks.acquire("k", Duration::seconds(10)).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn stale_guard_release_does_not_steal_new_holder() {
        let clock = Arc::new(FixedClock::at_unix_millis(0));
        let locks = manager_with(Arc::clone(&clock));

        let stale = locks.acquire("k", Duration::seconds(10)).await.unwrap().unwrap();
        clock.advance(Duration::seconds(11)); // stale's TTL passes

        // A new holder takes the key after expiry.
        let fresh = locks.acquire("k", Duration::seconds(10)).await.unwrap().unwrap();

        // Dropping the stale guard must NOT release the fresh holder's lock.
        drop(stale);
        assert!(locks.acquire("k", Duration::seconds(10)).await.unwrap().is_none());

        drop(fresh);
        assert!(locks.acquire("k", Duration::seconds(10)).await.unwrap().is_some());
    }
}
