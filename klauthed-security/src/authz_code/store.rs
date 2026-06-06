//! `AuthCodeStore` trait and in-memory implementation.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use klauthed_core::time::{Clock, SystemClock};

use crate::error::SecurityError;

use super::code::AuthCode;

// ── AuthCodeStore ─────────────────────────────────────────────────────────────

/// Async storage for short-lived [`AuthCode`]s.
///
/// The key contract is **single-use**: [`consume`](AuthCodeStore::consume)
/// atomically removes the entry so a replayed code returns `None`.
#[async_trait]
pub trait AuthCodeStore: Send + Sync {
    /// Persist an authorization code.
    ///
    /// # Errors
    /// Returns [`SecurityError`] on backend failure.
    async fn store(&self, code: AuthCode) -> Result<(), SecurityError>;

    /// Atomically look up **and remove** a code by its string value.
    ///
    /// Returns:
    /// * `Ok(Some(code))` — found, not expired, now removed.
    /// * `Ok(None)` — unknown, already consumed, or expired (lazily evicted).
    ///
    /// # Errors
    /// Returns [`SecurityError`] on backend failure.
    async fn consume(&self, code: &str) -> Result<Option<AuthCode>, SecurityError>;
}

// ── InMemoryAuthCodeStore ─────────────────────────────────────────────────────

/// An in-memory [`AuthCodeStore`] driven by an injected [`Clock`].
///
/// Cloneable handles share the same backing map. Inject a
/// [`FixedClock`](klauthed_core::time::FixedClock) in tests to control
/// expiry without sleeping.
#[derive(Clone)]
pub struct InMemoryAuthCodeStore {
    codes: Arc<Mutex<HashMap<String, AuthCode>>>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for InMemoryAuthCodeStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let len = self.codes.lock().map(|m| m.len()).unwrap_or(0);
        f.debug_struct("InMemoryAuthCodeStore")
            .field("codes", &len)
            .finish_non_exhaustive()
    }
}

impl Default for InMemoryAuthCodeStore {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryAuthCodeStore {
    /// A store backed by the real system clock.
    #[must_use]
    pub fn new() -> Self {
        Self::with_clock(Arc::new(SystemClock))
    }

    /// A store driven by a custom `clock` — inject a
    /// [`FixedClock`](klauthed_core::time::FixedClock) in tests.
    #[must_use]
    pub fn with_clock(clock: Arc<dyn Clock>) -> Self {
        Self {
            codes: Arc::new(Mutex::new(HashMap::new())),
            clock,
        }
    }

    /// Number of stored codes (including expired ones not yet evicted).
    #[must_use]
    pub fn len(&self) -> usize {
        self.codes.lock().expect("auth code store mutex poisoned").len()
    }

    /// Whether the store holds no codes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.codes.lock().expect("auth code store mutex poisoned").is_empty()
    }
}

#[async_trait]
impl AuthCodeStore for InMemoryAuthCodeStore {
    async fn store(&self, code: AuthCode) -> Result<(), SecurityError> {
        self.codes
            .lock()
            .expect("auth code store mutex poisoned")
            .insert(code.code.clone(), code);
        Ok(())
    }

    async fn consume(&self, code: &str) -> Result<Option<AuthCode>, SecurityError> {
        let now = self.clock.now();
        let mut map = self.codes.lock().expect("auth code store mutex poisoned");
        match map.remove(code) {
            // Expired: discard without re-inserting.
            Some(c) if c.is_expired(now) => Ok(None),
            found => Ok(found),
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use klauthed_core::time::{Duration, FixedClock};

    use super::super::code::AuthCodeBuilder;

    fn store_at(millis: i64) -> (Arc<FixedClock>, InMemoryAuthCodeStore) {
        let clock = Arc::new(FixedClock::at_unix_millis(millis));
        let store = InMemoryAuthCodeStore::with_clock(clock.clone());
        (clock, store)
    }

    fn code_for(client: &str, subject: &str, clock: &dyn Clock) -> AuthCode {
        AuthCodeBuilder::new(client, subject)
            .redirect_uri("https://app.example.com/cb")
            .scope(vec!["openid".into()])
            .build(clock, Duration::minutes(5))
            .unwrap()
    }

    #[tokio::test]
    async fn store_then_consume_returns_code_and_removes_it() {
        let (clock, store) = store_at(0);
        let code = code_for("c1", "alice", &*clock);
        let code_str = code.code.clone();

        store.store(code).await.unwrap();
        assert_eq!(store.len(), 1);

        let found = store.consume(&code_str).await.unwrap().unwrap();
        assert_eq!(found.subject, "alice");
        assert!(store.is_empty());
    }

    #[tokio::test]
    async fn second_consume_returns_none() {
        let (clock, store) = store_at(0);
        let code = code_for("c1", "alice", &*clock);
        let code_str = code.code.clone();

        store.store(code).await.unwrap();
        store.consume(&code_str).await.unwrap();
        assert!(store.consume(&code_str).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn consume_unknown_code_returns_none() {
        let (_clock, store) = store_at(0);
        assert!(store.consume("does-not-exist").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn consume_expired_code_evicts_and_returns_none() {
        let (clock, store) = store_at(0);
        let code = code_for("c1", "bob", &*clock);
        let code_str = code.code.clone();

        store.store(code).await.unwrap();
        clock.advance(Duration::minutes(6));

        assert!(store.consume(&code_str).await.unwrap().is_none());
        assert!(store.is_empty());
    }

    #[tokio::test]
    async fn cloned_handles_share_state() {
        let (clock, store) = store_at(0);
        let clone = store.clone();
        let code = code_for("c", "u", &*clock);
        let code_str = code.code.clone();

        store.store(code).await.unwrap();
        assert!(clone.consume(&code_str).await.unwrap().is_some());
        assert!(store.is_empty());
    }
}
