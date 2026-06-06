//! `RefreshTokenStore` trait and in-memory implementation with token-family
//! replay detection.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use klauthed_core::time::{Clock, SystemClock, Timestamp};

use crate::error::SecurityError;

use super::token::RefreshToken;

// ── ConsumeResult ─────────────────────────────────────────────────────────────

/// The outcome of consuming a refresh token.
#[derive(Debug)]
pub enum ConsumeResult {
    /// Valid, not expired. The token has been removed; issue a rotated one.
    Valid(RefreshToken),
    /// The token existed but has expired. A new token should NOT be issued.
    Expired(RefreshToken),
    /// The token was already consumed — replay attack detected. The token
    /// family has been revoked to limit blast radius.
    Compromised {
        /// The family whose tokens are now revoked.
        family_id: String,
    },
    /// Token is unknown: never stored, already expired and evicted, or the
    /// family was previously revoked.
    NotFound,
}

// ── RefreshTokenStore ─────────────────────────────────────────────────────────

/// Async storage for [`RefreshToken`]s.
///
/// The critical invariant is **single-use with replay detection**: consuming a
/// token removes it from the active set. If the same token is presented a
/// second time within its natural lifetime, the entire token family is revoked
/// (all live tokens for that client/user pair) and [`ConsumeResult::Compromised`]
/// is returned.
#[async_trait]
pub trait RefreshTokenStore: Send + Sync {
    /// Persist a newly issued (or rotated) refresh token.
    ///
    /// # Errors
    /// Returns [`SecurityError`] on backend failure.
    async fn store(&self, token: RefreshToken) -> Result<(), SecurityError>;

    /// Atomically look up and remove a refresh token by its bearer value.
    ///
    /// See [`ConsumeResult`] for the possible outcomes. Callers should act on
    /// [`ConsumeResult::Compromised`] by logging a security event.
    ///
    /// # Errors
    /// Returns [`SecurityError`] on backend failure.
    async fn consume(&self, token: &str) -> Result<ConsumeResult, SecurityError>;

    /// Revoke all active tokens belonging to `family_id`.
    ///
    /// Call this when you detect credential compromise outside the normal
    /// consume flow (e.g. a forced logout, account lock).
    ///
    /// # Errors
    /// Returns [`SecurityError`] on backend failure.
    async fn revoke_family(&self, family_id: &str) -> Result<(), SecurityError>;
}

// ── InMemoryRefreshTokenStore ─────────────────────────────────────────────────

/// Internal state for the in-memory store.
struct StoreState {
    /// Active (live, unconsumed) tokens indexed by bearer value.
    active: HashMap<String, RefreshToken>,
    /// Recently consumed tokens: `bearer_value → (family_id, original_expiry)`.
    ///
    /// Entries remain until the original token's natural expiry — after that,
    /// a replay can no longer be distinguished from an unknown token (which is
    /// fine because the token would fail expiry validation anyway).
    consumed: HashMap<String, (String, Timestamp)>,
    /// Family IDs known to be compromised; all tokens in these families are
    /// treated as revoked even if they appear in `active`.
    revoked_families: HashSet<String>,
}

impl StoreState {
    fn new() -> Self {
        Self {
            active: HashMap::new(),
            consumed: HashMap::new(),
            revoked_families: HashSet::new(),
        }
    }

    fn evict_expired_consumed(&mut self, now: Timestamp) {
        self.consumed.retain(|_, (_, expiry)| *expiry > now);
    }
}

/// An in-memory [`RefreshTokenStore`] with token-family replay detection.
///
/// Cloneable handles share the same backing state. Inject a
/// [`FixedClock`](klauthed_core::time::FixedClock) in tests.
#[derive(Clone)]
pub struct InMemoryRefreshTokenStore {
    state: Arc<Mutex<StoreState>>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for InMemoryRefreshTokenStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        f.debug_struct("InMemoryRefreshTokenStore")
            .field("active", &s.active.len())
            .field("consumed", &s.consumed.len())
            .field("revoked_families", &s.revoked_families.len())
            .finish_non_exhaustive()
    }
}

impl Default for InMemoryRefreshTokenStore {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryRefreshTokenStore {
    /// A store backed by the real system clock.
    #[must_use]
    pub fn new() -> Self {
        Self::with_clock(Arc::new(SystemClock))
    }

    /// A store driven by a custom `clock`.
    #[must_use]
    pub fn with_clock(clock: Arc<dyn Clock>) -> Self {
        Self {
            state: Arc::new(Mutex::new(StoreState::new())),
            clock,
        }
    }

    /// Number of active (live, unconsumed) tokens.
    #[must_use]
    pub fn active_count(&self) -> usize {
        self.state.lock().expect("mutex poisoned").active.len()
    }
}

#[async_trait]
impl RefreshTokenStore for InMemoryRefreshTokenStore {
    async fn store(&self, token: RefreshToken) -> Result<(), SecurityError> {
        let mut s = self.state.lock().expect("mutex poisoned");
        s.active.insert(token.token.clone(), token);
        Ok(())
    }

    async fn consume(&self, token: &str) -> Result<ConsumeResult, SecurityError> {
        let now = self.clock.now();
        let mut s = self.state.lock().expect("mutex poisoned");

        // ── 1. Is the family already revoked? ─────────────────────────────────
        // We need to find the family from the active map first.
        if let Some(rt) = s.active.get(token)
            && s.revoked_families.contains(&rt.family_id)
        {
            // Remove silently — the family is compromised.
            s.active.remove(token);
            return Ok(ConsumeResult::NotFound);
        }

        // ── 2. Token is in the active set ─────────────────────────────────────
        if let Some(rt) = s.active.remove(token) {
            // Record as consumed so a replay within the original window is detected.
            s.consumed
                .insert(token.to_owned(), (rt.family_id.clone(), rt.expires_at));

            if rt.is_expired(now) {
                return Ok(ConsumeResult::Expired(rt));
            }
            return Ok(ConsumeResult::Valid(rt));
        }

        // ── 3. Token was already consumed (replay detection) ──────────────────
        s.evict_expired_consumed(now);
        if let Some((family_id, _)) = s.consumed.get(token).cloned() {
            // Replay within the natural lifetime → compromise.
            s.revoked_families.insert(family_id.clone());
            // Evict all active tokens in this family.
            s.active.retain(|_, rt| rt.family_id != family_id);
            // Clear the specific consumed entry — the family revocation covers it.
            s.consumed.remove(token);
            return Ok(ConsumeResult::Compromised { family_id });
        }

        // ── 4. Truly unknown ──────────────────────────────────────────────────
        Ok(ConsumeResult::NotFound)
    }

    async fn revoke_family(&self, family_id: &str) -> Result<(), SecurityError> {
        let mut s = self.state.lock().expect("mutex poisoned");
        s.revoked_families.insert(family_id.to_owned());
        s.active.retain(|_, rt| rt.family_id != family_id);
        Ok(())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use klauthed_core::time::{Duration, FixedClock};

    use super::super::token::RefreshTokenBuilder;

    fn store_at(millis: i64) -> (Arc<FixedClock>, InMemoryRefreshTokenStore) {
        let clock = Arc::new(FixedClock::at_unix_millis(millis));
        let store = InMemoryRefreshTokenStore::with_clock(clock.clone());
        (clock, store)
    }

    fn token_for(client: &str, subject: &str, clock: &dyn Clock) -> RefreshToken {
        RefreshTokenBuilder::new(client, subject)
            .scope(vec!["openid".into()])
            .build(clock, Duration::days(30))
            .unwrap()
    }

    #[tokio::test]
    async fn store_then_consume_returns_valid() {
        let (clock, store) = store_at(0);
        let rt = token_for("c", "alice", &*clock);
        let token_str = rt.token.clone();

        store.store(rt).await.unwrap();
        assert_eq!(store.active_count(), 1);

        let result = store.consume(&token_str).await.unwrap();
        assert!(matches!(result, ConsumeResult::Valid(_)));
        assert_eq!(store.active_count(), 0);
    }

    #[tokio::test]
    async fn replay_within_lifetime_returns_compromised() {
        let (clock, store) = store_at(0);
        let rt = token_for("c", "bob", &*clock);
        let token_str = rt.token.clone();
        let family_id = rt.family_id.clone();

        store.store(rt).await.unwrap();
        // First consume: OK
        let r1 = store.consume(&token_str).await.unwrap();
        assert!(matches!(r1, ConsumeResult::Valid(_)));

        // Replay within the 30-day window: compromised
        let r2 = store.consume(&token_str).await.unwrap();
        match r2 {
            ConsumeResult::Compromised { family_id: fid } => {
                assert_eq!(fid, family_id);
            }
            other => panic!("expected Compromised, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn expired_token_returns_expired() {
        let (clock, store) = store_at(0);
        let rt = RefreshTokenBuilder::new("c", "carol")
            .build(&*clock, Duration::seconds(10))
            .unwrap();
        let token_str = rt.token.clone();
        store.store(rt).await.unwrap();

        // Advance past expiry.
        clock.advance(Duration::seconds(11));

        let result = store.consume(&token_str).await.unwrap();
        assert!(matches!(result, ConsumeResult::Expired(_)));
    }

    #[tokio::test]
    async fn compromise_revokes_all_active_family_tokens() {
        let (clock, store) = store_at(0);

        // Issue two tokens in the same family.
        let rt1 = token_for("c", "dave", &*clock);
        let family_id = rt1.family_id.clone();
        let rt2 = RefreshTokenBuilder::new("c", "dave")
            .family_id(&family_id)
            .build(&*clock, Duration::days(30))
            .unwrap();

        let token1_str = rt1.token.clone();
        let token2_str = rt2.token.clone();

        store.store(rt1).await.unwrap();
        store.store(rt2).await.unwrap();
        assert_eq!(store.active_count(), 2);

        // Consume the first, then replay it.
        store.consume(&token1_str).await.unwrap();
        let r = store.consume(&token1_str).await.unwrap();
        assert!(matches!(r, ConsumeResult::Compromised { .. }));

        // The second token in the same family is now gone.
        assert_eq!(store.active_count(), 0);
        let r2 = store.consume(&token2_str).await.unwrap();
        assert!(matches!(r2, ConsumeResult::NotFound));
    }

    #[tokio::test]
    async fn revoke_family_removes_all_active_tokens() {
        let (clock, store) = store_at(0);
        let rt1 = token_for("c", "eve", &*clock);
        let family_id = rt1.family_id.clone();
        let rt2 = RefreshTokenBuilder::new("c", "eve")
            .family_id(&family_id)
            .build(&*clock, Duration::days(30))
            .unwrap();
        let token2_str = rt2.token.clone();

        store.store(rt1).await.unwrap();
        store.store(rt2).await.unwrap();

        store.revoke_family(&family_id).await.unwrap();
        assert_eq!(store.active_count(), 0);
        assert!(matches!(
            store.consume(&token2_str).await.unwrap(),
            ConsumeResult::NotFound
        ));
    }

    #[tokio::test]
    async fn consume_unknown_token_returns_not_found() {
        let (_clock, store) = store_at(0);
        assert!(matches!(
            store.consume("does-not-exist").await.unwrap(),
            ConsumeResult::NotFound
        ));
    }

    #[tokio::test]
    async fn cloned_handles_share_state() {
        let (clock, store) = store_at(0);
        let clone = store.clone();
        let rt = token_for("c", "frank", &*clock);
        let token_str = rt.token.clone();

        store.store(rt).await.unwrap();
        let result = clone.consume(&token_str).await.unwrap();
        assert!(matches!(result, ConsumeResult::Valid(_)));
        assert_eq!(store.active_count(), 0);
    }
}
