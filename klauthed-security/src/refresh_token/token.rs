//! `RefreshToken` model and builder.

use klauthed_core::time::{Clock, Duration, Timestamp};

use crate::error::SecurityError;
use crate::token::random_token;

/// Bytes of entropy in a freshly minted refresh token (256 bits).
const REFRESH_TOKEN_BYTES: usize = 32;

// ── RefreshToken ──────────────────────────────────────────────────────────────

/// A long-lived bearer credential that can be exchanged for a new access token.
///
/// # Token families
///
/// Every refresh token belongs to a `family_id` that is stable across
/// rotations. If an already-consumed token is presented again (replay), the
/// entire family is revoked — this detects a stolen refresh token that was
/// used after the legitimate holder already rotated it.
///
/// # Security
///
/// The `token` field is the raw secret bearer value. Treat it like a password
/// — never log it. Only compare it via
/// [`RefreshTokenStore::consume`](super::RefreshTokenStore::consume).
#[derive(Debug, Clone)]
pub struct RefreshToken {
    /// The opaque bearer value (URL-safe base64, 256 bits of entropy).
    pub token: String,
    /// Stable identifier for the token's rotation chain. Preserved across
    /// rotations; newly minted families use a fresh random value.
    pub family_id: String,
    /// The client that requested this token.
    pub client_id: String,
    /// The authenticated user this token is issued for.
    pub subject: String,
    /// The scopes granted to this token.
    pub scope: Vec<String>,
    /// When this token was issued.
    pub issued_at: Timestamp,
    /// When this token expires.
    pub expires_at: Timestamp,
}

impl RefreshToken {
    /// Return `true` if this token has expired as of `now`.
    #[must_use]
    pub fn is_expired(&self, now: Timestamp) -> bool {
        now >= self.expires_at
    }
}

// ── RefreshTokenBuilder ───────────────────────────────────────────────────────

/// Fluent builder for [`RefreshToken`].
pub struct RefreshTokenBuilder {
    client_id: String,
    subject: String,
    scope: Vec<String>,
    /// If `None`, a fresh random family id is minted (new token family).
    family_id: Option<String>,
}

impl RefreshTokenBuilder {
    /// Start building a refresh token for `client_id` / `subject`.
    pub fn new(client_id: impl Into<String>, subject: impl Into<String>) -> Self {
        Self {
            client_id: client_id.into(),
            subject: subject.into(),
            scope: Vec::new(),
            family_id: None,
        }
    }

    /// Set the granted scopes.
    #[must_use]
    pub fn scope(mut self, scope: Vec<String>) -> Self {
        self.scope = scope;
        self
    }

    /// Inherit `family_id` from the parent token (rotation — keeps the same
    /// family). Omit this call to start a new family (initial issuance).
    #[must_use]
    pub fn family_id(mut self, id: impl Into<String>) -> Self {
        self.family_id = Some(id.into());
        self
    }

    /// Mint the [`RefreshToken`] using `clock` for timestamps.
    ///
    /// # Errors
    /// Returns [`SecurityError::Rng`] if the OS CSPRNG fails.
    pub fn build<C: Clock + ?Sized>(
        self,
        clock: &C,
        ttl: Duration,
    ) -> Result<RefreshToken, SecurityError> {
        let token = random_token(REFRESH_TOKEN_BYTES)?;
        let family_id = match self.family_id {
            Some(id) => id,
            None => random_token(REFRESH_TOKEN_BYTES)?, // fresh family
        };
        let now = clock.now();
        let expires_at = now
            .checked_add(ttl)
            .ok_or_else(|| SecurityError::TokenTtlOverflow("refresh token".into()))?;

        Ok(RefreshToken {
            token,
            family_id,
            client_id: self.client_id,
            subject: self.subject,
            scope: self.scope,
            issued_at: now,
            expires_at,
        })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use klauthed_core::time::{FixedClock, SystemClock, Timestamp};

    #[test]
    fn builder_creates_token_with_fresh_family() {
        let clock = FixedClock::at_unix_millis(0);
        let token = RefreshTokenBuilder::new("client", "alice")
            .scope(vec!["openid".into()])
            .build(&clock, Duration::days(30))
            .unwrap();

        assert_eq!(token.client_id, "client");
        assert_eq!(token.subject, "alice");
        assert_eq!(token.scope, ["openid"]);
        assert!(!token.token.is_empty());
        assert!(!token.family_id.is_empty());
    }

    #[test]
    fn rotation_preserves_family_id() {
        let clock = SystemClock;
        let first = RefreshTokenBuilder::new("c", "u")
            .build(&clock, Duration::days(30))
            .unwrap();
        let rotated = RefreshTokenBuilder::new("c", "u")
            .family_id(&first.family_id)
            .build(&clock, Duration::days(30))
            .unwrap();

        assert_eq!(first.family_id, rotated.family_id);
        assert_ne!(first.token, rotated.token); // different bearer value
    }

    #[test]
    fn tokens_are_unique() {
        let clock = SystemClock;
        let a = RefreshTokenBuilder::new("c", "u")
            .build(&clock, Duration::days(30))
            .unwrap();
        let b = RefreshTokenBuilder::new("c", "u")
            .build(&clock, Duration::days(30))
            .unwrap();
        assert_ne!(a.token, b.token);
        assert_ne!(a.family_id, b.family_id);
    }

    #[test]
    fn is_expired_checks_expiry() {
        let clock = FixedClock::at_unix_millis(0);
        let token = RefreshTokenBuilder::new("c", "u")
            .build(&clock, Duration::days(1))
            .unwrap();

        let before = Timestamp::from_unix_millis(1_000);
        let after = Timestamp::from_unix_millis(24 * 60 * 60 * 1000 + 1);
        assert!(!token.is_expired(before));
        assert!(token.is_expired(after));
    }
}
