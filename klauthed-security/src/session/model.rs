//! Session value types: the opaque [`SessionId`] and the [`Session`] record.

use std::collections::HashMap;

use klauthed_core::time::Timestamp;

use crate::error::SecurityError;
use crate::token::random_token;

/// Bytes of entropy in a freshly minted session id (256 bits).
const SESSION_ID_BYTES: usize = 32;

/// An opaque, unguessable session identifier.
///
/// This is a newtype over the URL-safe base64 token string (not a UUID): the id
/// *is* the secret bearer credential, so it carries full CSPRNG entropy rather
/// than a structured/sortable id. Treat it like a password — compare it only via
/// the store, never log it.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SessionId(String);

impl std::fmt::Debug for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The id is a bearer credential — never print it (even via a derived
        // Debug on a containing type such as `Session`).
        f.write_str("SessionId(***)")
    }
}

impl SessionId {
    /// Mint a fresh random session id (256 bits of entropy).
    ///
    /// # Errors
    /// Returns [`SecurityError::Rng`] if the OS CSPRNG fails.
    pub fn generate() -> Result<Self, SecurityError> {
        Ok(Self(random_token(SESSION_ID_BYTES)?))
    }

    /// Wrap an existing token string as a session id (e.g. one read from a
    /// cookie). No validation beyond being a string is performed.
    pub fn from_token(token: impl Into<String>) -> Self {
        Self(token.into())
    }

    /// The id as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume into the owned token string.
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A server-side session: who it belongs to and when it expires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    /// The opaque bearer id for this session.
    pub id: SessionId,
    /// The principal (subject) this session authenticates, e.g. a user id.
    pub subject: String,
    /// When the session was created.
    pub created_at: Timestamp,
    /// When the session expires; at or after this instant it is invalid.
    pub expires_at: Timestamp,
    /// Arbitrary application metadata (device, ip, roles snapshot, …).
    pub metadata: HashMap<String, String>,
}

impl Session {
    /// Whether the session is expired as of `now` (expiry is inclusive: a
    /// session is expired once `now >= expires_at`).
    #[must_use]
    pub fn is_expired(&self, now: Timestamp) -> bool {
        now >= self.expires_at
    }
}
