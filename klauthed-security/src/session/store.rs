//! Session storage: the [`SessionStore`] trait and the in-memory
//! [`InMemorySessionStore`].

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use klauthed_core::time::{Clock, Duration, SystemClock, Timestamp};

use crate::error::SecurityError;

use super::{Session, SessionId};

/// Storage for [`Session`]s.
///
/// `get` is defined to return `None` for an expired (or absent) session, so
/// callers never have to re-check expiry themselves. Implementations are
/// `Send + Sync` and use `async` so DB/Redis backends fit the same seam.
#[async_trait]
pub trait SessionStore: Send + Sync {
    /// Create and persist a new session for `subject`, valid for `ttl` from the
    /// store's current clock. Returns the stored [`Session`] (including its id).
    ///
    /// # Errors
    /// Returns a [`SecurityError`] if the id could not be generated or the
    /// expiry could not be computed.
    async fn create(
        &self,
        subject: &str,
        ttl: Duration,
        metadata: Option<HashMap<String, String>>,
    ) -> Result<Session, SecurityError>;

    /// Fetch a session by id, returning `None` if it is unknown **or expired**.
    ///
    /// # Errors
    /// Returns a [`SecurityError`] only on backend failure (the in-memory store
    /// is infallible here).
    async fn get(&self, id: &SessionId) -> Result<Option<Session>, SecurityError>;

    /// Delete a session (idempotent: deleting an unknown id is `Ok`).
    ///
    /// # Errors
    /// Returns a [`SecurityError`] only on backend failure.
    async fn delete(&self, id: &SessionId) -> Result<(), SecurityError>;

    /// Extend a live session's expiry to now + `ttl` ("sliding" sessions).
    ///
    /// Returns the updated session, or [`SecurityError::SessionNotFound`] /
    /// [`SecurityError::SessionExpired`] if it is gone or already expired.
    ///
    /// # Errors
    /// As above, plus any backend failure.
    async fn touch(&self, id: &SessionId, ttl: Duration) -> Result<Session, SecurityError>;
}

/// A thread-safe, in-memory [`SessionStore`] driven by an injected [`Clock`].
///
/// Expiry is decided from the clock, so tests can pin/advance time via
/// [`FixedClock`](klauthed_core::time::FixedClock). Cloneable handles share one
/// backing map (`Arc<Mutex<…>>`).
#[derive(Clone)]
pub struct InMemorySessionStore {
    sessions: Arc<Mutex<HashMap<SessionId, Session>>>,
    clock: Arc<dyn Clock>,
}

impl Default for InMemorySessionStore {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for InMemorySessionStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let len = self.sessions.lock().map(|m| m.len()).unwrap_or(0);
        f.debug_struct("InMemorySessionStore").field("sessions", &len).finish_non_exhaustive()
    }
}

impl InMemorySessionStore {
    /// A store backed by the real [`SystemClock`].
    #[must_use]
    pub fn new() -> Self {
        Self::with_clock(Arc::new(SystemClock))
    }

    /// A store driven by `clock` (inject a
    /// [`FixedClock`](klauthed_core::time::FixedClock) in tests).
    #[must_use]
    pub fn with_clock(clock: Arc<dyn Clock>) -> Self {
        Self { sessions: Arc::new(Mutex::new(HashMap::new())), clock }
    }

    /// Number of stored sessions (including any not-yet-evicted expired ones).
    #[must_use]
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    /// Whether the store holds no sessions.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<SessionId, Session>> {
        self.sessions.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// now + `ttl`, as a [`Timestamp`], erroring on overflow.
    fn deadline(&self, ttl: Duration) -> Result<Timestamp, SecurityError> {
        self.clock
            .now()
            .checked_add(ttl)
            .ok_or_else(|| SecurityError::TokenTtlOverflow("session".to_owned()))
    }
}

#[async_trait]
impl SessionStore for InMemorySessionStore {
    async fn create(
        &self,
        subject: &str,
        ttl: Duration,
        metadata: Option<HashMap<String, String>>,
    ) -> Result<Session, SecurityError> {
        let now = self.clock.now();
        let session = Session {
            id: SessionId::generate()?,
            subject: subject.to_owned(),
            created_at: now,
            expires_at: self.deadline(ttl)?,
            metadata: metadata.unwrap_or_default(),
        };
        self.lock().insert(session.id.clone(), session.clone());
        Ok(session)
    }

    async fn get(&self, id: &SessionId) -> Result<Option<Session>, SecurityError> {
        let now = self.clock.now();
        let mut map = self.lock();
        match map.get(id) {
            Some(s) if s.is_expired(now) => {
                // Lazily evict expired sessions on access.
                map.remove(id);
                Ok(None)
            }
            Some(s) => Ok(Some(s.clone())),
            None => Ok(None),
        }
    }

    async fn delete(&self, id: &SessionId) -> Result<(), SecurityError> {
        self.lock().remove(id);
        Ok(())
    }

    async fn touch(&self, id: &SessionId, ttl: Duration) -> Result<Session, SecurityError> {
        let now = self.clock.now();
        let new_deadline = self.deadline(ttl)?;
        let mut map = self.lock();
        match map.get_mut(id) {
            Some(s) if s.is_expired(now) => {
                map.remove(id);
                Err(SecurityError::SessionExpired)
            }
            Some(s) => {
                s.expires_at = new_deadline;
                Ok(s.clone())
            }
            None => Err(SecurityError::SessionNotFound),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use klauthed_core::time::FixedClock;
    use klauthed_error::{DomainError, ErrorCategory};

    fn store_at(millis: i64) -> (Arc<FixedClock>, InMemorySessionStore) {
        let clock = Arc::new(FixedClock::at_unix_millis(millis));
        let store = InMemorySessionStore::with_clock(clock.clone());
        (clock, store)
    }

    #[tokio::test]
    async fn create_then_get_returns_session() {
        let (_clock, store) = store_at(0);
        let s = store.create("alice", Duration::minutes(30), None).await.unwrap();
        let got = store.get(&s.id).await.unwrap().unwrap();
        assert_eq!(got.subject, "alice");
        assert_eq!(got.id, s.id);
    }

    #[tokio::test]
    async fn get_returns_none_after_expiry() {
        let (clock, store) = store_at(0);
        let s = store.create("bob", Duration::seconds(30), None).await.unwrap();
        assert!(store.get(&s.id).await.unwrap().is_some());

        clock.advance(Duration::seconds(31));
        assert!(store.get(&s.id).await.unwrap().is_none());
        // Lazily evicted.
        assert!(store.is_empty());
    }

    #[tokio::test]
    async fn touch_extends_expiry() {
        let (clock, store) = store_at(0);
        let s = store.create("carol", Duration::seconds(30), None).await.unwrap();

        clock.advance(Duration::seconds(20));
        let extended = store.touch(&s.id, Duration::seconds(30)).await.unwrap();
        assert!(extended.expires_at > s.expires_at);

        // 25s after the original 30s deadline, but touch reset it.
        clock.advance(Duration::seconds(25));
        assert!(store.get(&s.id).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn touch_expired_session_errors() {
        let (clock, store) = store_at(0);
        let s = store.create("dave", Duration::seconds(10), None).await.unwrap();
        clock.advance(Duration::seconds(11));
        let err = store.touch(&s.id, Duration::seconds(30)).await.unwrap_err();
        assert!(matches!(err, SecurityError::SessionExpired));
        assert_eq!(err.category(), ErrorCategory::Unauthorized);
        assert_eq!(err.code().as_str(), "security.session_expired");
    }

    #[tokio::test]
    async fn delete_is_idempotent_and_removes() {
        let (_clock, store) = store_at(0);
        let s = store.create("erin", Duration::minutes(5), None).await.unwrap();
        store.delete(&s.id).await.unwrap();
        assert!(store.get(&s.id).await.unwrap().is_none());
        // Deleting again is fine.
        store.delete(&s.id).await.unwrap();
    }

    #[tokio::test]
    async fn touch_missing_session_is_not_found() {
        let (_clock, store) = store_at(0);
        let missing = SessionId::from_token("does-not-exist");
        let err = store.touch(&missing, Duration::seconds(30)).await.unwrap_err();
        assert!(matches!(err, SecurityError::SessionNotFound));
        assert_eq!(err.category(), ErrorCategory::NotFound);
    }

    #[tokio::test]
    async fn metadata_round_trips() {
        let (_clock, store) = store_at(0);
        let mut meta = HashMap::new();
        meta.insert("device".to_owned(), "cli".to_owned());
        let s = store.create("frank", Duration::minutes(5), Some(meta)).await.unwrap();
        let got = store.get(&s.id).await.unwrap().unwrap();
        assert_eq!(got.metadata.get("device").map(String::as_str), Some("cli"));
    }

    #[test]
    fn session_ids_are_unique_and_url_safe() {
        let a = SessionId::generate().unwrap();
        let b = SessionId::generate().unwrap();
        assert_ne!(a, b);
        assert!(a.as_str().bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'));
    }
}
