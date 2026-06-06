//! Token revocation: a denylist for JWT `jti` values.
//!
//! When a token is revoked (user logout, credential rotation, compromise
//! detection), its `jti` claim is inserted into the [`TokenDenylist`] with the
//! token's original `exp` as the expiry. The denylist self-prunes: once a
//! token's natural lifetime has passed, its entry is lazily evicted on the next
//! [`is_revoked`](TokenDenylist::is_revoked) call because an expired token
//! would fail `exp` validation in [`JwtVerifier`](crate::JwtVerifier) anyway.
//!
//! [`TokenDenylist`] is the async storage trait.
//! [`InMemoryTokenDenylist`] provides a clock-injected, in-memory
//! implementation suitable for tests and single-replica deployments.
//!
//! # Integration
//!
//! After decoding a token with [`JwtVerifier::decode`](crate::JwtVerifier::decode),
//! check the `jti` claim against the denylist before admitting the request:
//!
//! ```
//! use std::sync::Arc;
//! use klauthed_core::time::{FixedClock, Timestamp};
//! use klauthed_security::revocation::{InMemoryTokenDenylist, TokenDenylist};
//!
//! # #[tokio::main]
//! # async fn main() {
//! let denylist = InMemoryTokenDenylist::new();
//! let jti = "unique-token-id-abc";
//! let expires_at = Timestamp::from_unix_millis(9_999_999_999_000); // ~year 2286
//!
//! // Token is live.
//! assert!(!denylist.is_revoked(jti).await.unwrap());
//!
//! // Revoke it.
//! denylist.revoke(jti.into(), expires_at).await.unwrap();
//! assert!(denylist.is_revoked(jti).await.unwrap());
//! # }
//! ```

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use klauthed_core::time::{Clock, SystemClock, Timestamp};

use crate::error::SecurityError;

// ── TokenDenylist ─────────────────────────────────────────────────────────────

/// Async storage for revoked JWT `jti` values.
///
/// Implementations must be `Send + Sync` so they can be held as
/// `Arc<dyn TokenDenylist>` and shared across worker threads.
///
/// # Expiry contract
///
/// Callers should pass `expires_at` equal to the token's own `exp` claim (as a
/// [`Timestamp`]). The denylist uses this to know when the entry can be pruned
/// — once the token is naturally expired, the denylist entry is redundant.
#[async_trait]
pub trait TokenDenylist: Send + Sync {
    /// Mark `jti` as revoked, with the entry expiring at `expires_at`.
    ///
    /// Revoking an already-revoked `jti` is idempotent; the expiry is updated
    /// to the new value.
    ///
    /// # Errors
    /// Returns [`SecurityError`] on backend failure.
    async fn revoke(&self, jti: String, expires_at: Timestamp) -> Result<(), SecurityError>;

    /// Return `true` if `jti` is in the denylist and its entry has not yet
    /// expired.
    ///
    /// A `false` result means either the token was never revoked, or its
    /// denylist entry has expired (the token would also fail `exp` validation
    /// at the verifier, making the check redundant).
    ///
    /// # Errors
    /// Returns [`SecurityError`] on backend failure.
    async fn is_revoked(&self, jti: &str) -> Result<bool, SecurityError>;
}

// ── InMemoryTokenDenylist ─────────────────────────────────────────────────────

/// An in-memory [`TokenDenylist`] backed by a `Mutex<HashMap>`.
///
/// Cloneable handles share the same backing map (`Arc<Mutex<…>>`). Inject a
/// [`FixedClock`](klauthed_core::time::FixedClock) in tests to control expiry
/// deterministically.
///
/// Expired entries are lazily evicted on each [`is_revoked`] call to prevent
/// unbounded growth. For production deployments with many short-lived tokens,
/// prefer a Redis-backed implementation that handles TTL natively.
///
/// [`is_revoked`]: InMemoryTokenDenylist::is_revoked
#[derive(Clone)]
pub struct InMemoryTokenDenylist {
    entries: Arc<Mutex<HashMap<String, Timestamp>>>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for InMemoryTokenDenylist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let len = self.entries.lock().map(|m| m.len()).unwrap_or(0);
        f.debug_struct("InMemoryTokenDenylist")
            .field("entries", &len)
            .finish_non_exhaustive()
    }
}

impl Default for InMemoryTokenDenylist {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryTokenDenylist {
    /// A denylist backed by the real system clock.
    #[must_use]
    pub fn new() -> Self {
        Self::with_clock(Arc::new(SystemClock))
    }

    /// A denylist driven by `clock`.
    #[must_use]
    pub fn with_clock(clock: Arc<dyn Clock>) -> Self {
        Self {
            entries: Arc::new(Mutex::new(HashMap::new())),
            clock,
        }
    }

    /// Number of entries currently in the denylist (including expired ones not
    /// yet evicted).
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.lock().expect("denylist mutex poisoned").len()
    }

    /// Whether the denylist has no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.lock().expect("denylist mutex poisoned").is_empty()
    }
}

#[async_trait]
impl TokenDenylist for InMemoryTokenDenylist {
    async fn revoke(&self, jti: String, expires_at: Timestamp) -> Result<(), SecurityError> {
        self.entries
            .lock()
            .expect("denylist mutex poisoned")
            .insert(jti, expires_at);
        Ok(())
    }

    async fn is_revoked(&self, jti: &str) -> Result<bool, SecurityError> {
        let now = self.clock.now();
        let mut map = self.entries.lock().expect("denylist mutex poisoned");
        match map.get(jti).copied() {
            Some(expires_at) if expires_at <= now => {
                // The entry has expired — lazily evict and report as not revoked.
                map.remove(jti);
                Ok(false)
            }
            Some(_) => Ok(true),
            None => Ok(false),
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use klauthed_core::time::{Duration, FixedClock};

    fn denylist_at(millis: i64) -> (Arc<FixedClock>, InMemoryTokenDenylist) {
        let clock = Arc::new(FixedClock::at_unix_millis(millis));
        let list = InMemoryTokenDenylist::with_clock(clock.clone());
        (clock, list)
    }

    /// A timestamp roughly 10 years out — well within the representable range.
    fn far_future() -> Timestamp {
        Timestamp::from_unix_millis(9_999_999_999_000) // ~year 2286
    }

    #[tokio::test]
    async fn not_revoked_before_any_entry() {
        let (_clock, list) = denylist_at(0);
        assert!(!list.is_revoked("jti-1").await.unwrap());
    }

    #[tokio::test]
    async fn revoked_token_is_detected() {
        let (_clock, list) = denylist_at(0);
        list.revoke("jti-1".into(), far_future()).await.unwrap();
        assert!(list.is_revoked("jti-1").await.unwrap());
    }

    #[tokio::test]
    async fn other_jtis_are_unaffected() {
        let (_clock, list) = denylist_at(0);
        list.revoke("jti-a".into(), far_future()).await.unwrap();
        assert!(!list.is_revoked("jti-b").await.unwrap());
    }

    #[tokio::test]
    async fn expired_entry_is_evicted_and_reported_as_not_revoked() {
        let (clock, list) = denylist_at(0);
        // Revoke with an expiry 30 seconds from now.
        let expires_at = clock.now().checked_add(Duration::seconds(30)).unwrap();
        list.revoke("jti-x".into(), expires_at).await.unwrap();

        assert!(list.is_revoked("jti-x").await.unwrap());
        assert_eq!(list.len(), 1);

        // Advance past the entry's expiry.
        clock.advance(Duration::seconds(31));

        assert!(!list.is_revoked("jti-x").await.unwrap());
        assert!(list.is_empty()); // lazily evicted
    }

    #[tokio::test]
    async fn revoking_same_jti_twice_updates_expiry() {
        let (_clock, list) = denylist_at(0);
        let exp1 = Timestamp::from_unix_millis(1_000_000);
        let exp2 = far_future();

        list.revoke("jti".into(), exp1).await.unwrap();
        list.revoke("jti".into(), exp2).await.unwrap();

        assert_eq!(list.len(), 1);
        assert!(list.is_revoked("jti").await.unwrap());
    }

    #[tokio::test]
    async fn cloned_lists_share_state() {
        let (_clock, list) = denylist_at(0);
        let clone = list.clone();

        list.revoke("jti".into(), far_future()).await.unwrap();
        assert!(clone.is_revoked("jti").await.unwrap());
    }
}
