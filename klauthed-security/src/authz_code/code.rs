//! Authorization code model: `PkceMethod`, `AuthCode`, and `AuthCodeBuilder`.

use serde::{Deserialize, Serialize};

use klauthed_core::time::{Clock, Duration, Timestamp};

use crate::error::SecurityError;
use crate::token::random_token;

/// Bytes of entropy in a freshly minted authorization code (256 bits).
const AUTH_CODE_BYTES: usize = 32;

// ── PkceMethod ────────────────────────────────────────────────────────────────

/// PKCE code challenge method (RFC 7636 §4.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PkceMethod {
    /// `plain` — the code verifier equals the challenge directly.
    Plain,
    /// `S256` — the challenge is `BASE64URL-NOPAD(SHA256(verifier))`.
    S256,
}

// ── AuthCode ──────────────────────────────────────────────────────────────────

/// A short-lived, single-use authorization code (RFC 6749 §4.1.2).
///
/// The `code` field is the opaque bearer value sent to the client's
/// `redirect_uri`. Treat it like a password — never log it.
#[derive(Debug, Clone)]
pub struct AuthCode {
    /// The opaque code value (URL-safe base64, 256 bits of entropy).
    pub code: String,
    /// The client that initiated the authorization request.
    pub client_id: String,
    /// The authenticated user this code is issued for.
    pub subject: String,
    /// The `redirect_uri` from the authorization request, if provided.
    pub redirect_uri: Option<String>,
    /// The scopes granted (split from the space-delimited wire form).
    pub scope: Vec<String>,
    /// OIDC `nonce` value, if the client provided one.
    pub nonce: Option<String>,
    /// PKCE `code_challenge` (RFC 7636), if the client provided one.
    pub pkce_challenge: Option<String>,
    /// The PKCE challenge method.
    pub pkce_method: Option<PkceMethod>,
    /// When the code was issued.
    pub issued_at: Timestamp,
    /// When the code expires (≤ 10 minutes per RFC 6749 §4.1.2).
    pub expires_at: Timestamp,
}

impl AuthCode {
    /// Return `true` if this code has expired as of `now`.
    #[must_use]
    pub fn is_expired(&self, now: Timestamp) -> bool {
        now >= self.expires_at
    }
}

// ── AuthCodeBuilder ───────────────────────────────────────────────────────────

/// Fluent builder for [`AuthCode`].
///
/// Supply the mandatory `client_id` and `subject`, chain optional fields,
/// then call [`build`](AuthCodeBuilder::build).
pub struct AuthCodeBuilder {
    client_id: String,
    subject: String,
    redirect_uri: Option<String>,
    scope: Vec<String>,
    nonce: Option<String>,
    pkce_challenge: Option<String>,
    pkce_method: Option<PkceMethod>,
}

impl AuthCodeBuilder {
    /// Start building a code for `client_id` / `subject`.
    pub fn new(client_id: impl Into<String>, subject: impl Into<String>) -> Self {
        Self {
            client_id: client_id.into(),
            subject: subject.into(),
            redirect_uri: None,
            scope: Vec::new(),
            nonce: None,
            pkce_challenge: None,
            pkce_method: None,
        }
    }

    /// Set the redirect URI from the authorization request.
    #[must_use]
    pub fn redirect_uri(mut self, uri: impl Into<String>) -> Self {
        self.redirect_uri = Some(uri.into());
        self
    }

    /// Set the granted scopes.
    #[must_use]
    pub fn scope(mut self, scope: Vec<String>) -> Self {
        self.scope = scope;
        self
    }

    /// Set the OIDC nonce.
    #[must_use]
    pub fn nonce(mut self, nonce: impl Into<String>) -> Self {
        self.nonce = Some(nonce.into());
        self
    }

    /// Set the PKCE challenge and method.
    #[must_use]
    pub fn pkce(mut self, challenge: impl Into<String>, method: PkceMethod) -> Self {
        self.pkce_challenge = Some(challenge.into());
        self.pkce_method = Some(method);
        self
    }

    /// Mint the [`AuthCode`] using `clock` for timestamps and `ttl` for expiry.
    ///
    /// Per RFC 6749 §4.1.2, use a `ttl` of at most 10 minutes in production.
    ///
    /// # Errors
    /// Returns [`SecurityError::Rng`] if the OS CSPRNG fails.
    pub fn build<C: Clock + ?Sized>(
        self,
        clock: &C,
        ttl: Duration,
    ) -> Result<AuthCode, SecurityError> {
        let code = random_token(AUTH_CODE_BYTES)?;
        let now = clock.now();
        let expires_at = now
            .checked_add(ttl)
            .ok_or_else(|| SecurityError::TokenTtlOverflow("auth code".into()))?;
        Ok(AuthCode {
            code,
            client_id: self.client_id,
            subject: self.subject,
            redirect_uri: self.redirect_uri,
            scope: self.scope,
            nonce: self.nonce,
            pkce_challenge: self.pkce_challenge,
            pkce_method: self.pkce_method,
            issued_at: now,
            expires_at,
        })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use klauthed_core::time::{FixedClock, SystemClock};

    #[test]
    fn builder_sets_all_fields() {
        let clock = FixedClock::at_unix_millis(0);
        let code = AuthCodeBuilder::new("client", "user")
            .redirect_uri("https://example.com/cb")
            .scope(vec!["openid".into()])
            .nonce("n-xyz")
            .pkce("E9Melhoa2...", PkceMethod::S256)
            .build(&clock, Duration::minutes(5))
            .unwrap();

        assert_eq!(code.client_id, "client");
        assert_eq!(code.subject, "user");
        assert_eq!(code.redirect_uri.as_deref(), Some("https://example.com/cb"));
        assert_eq!(code.scope, vec!["openid"]);
        assert_eq!(code.nonce.as_deref(), Some("n-xyz"));
        assert_eq!(code.pkce_challenge.as_deref(), Some("E9Melhoa2..."));
        assert_eq!(code.pkce_method, Some(PkceMethod::S256));
        assert!(!code.code.is_empty());
    }

    #[test]
    fn codes_are_unique_and_url_safe() {
        let clock = SystemClock;
        let a = AuthCodeBuilder::new("c", "u")
            .build(&clock, Duration::minutes(5))
            .unwrap();
        let b = AuthCodeBuilder::new("c", "u")
            .build(&clock, Duration::minutes(5))
            .unwrap();
        assert_ne!(a.code, b.code);
        assert!(a
            .code
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'));
    }

    #[test]
    fn is_expired_checks_expiry() {
        let clock = FixedClock::at_unix_millis(0);
        let code = AuthCodeBuilder::new("c", "u")
            .build(&clock, Duration::minutes(5))
            .unwrap();
        let before = klauthed_core::time::Timestamp::from_unix_millis(1_000);
        let after =
            klauthed_core::time::Timestamp::from_unix_millis(5 * 60 * 1000 + 1);
        assert!(!code.is_expired(before));
        assert!(code.is_expired(after));
    }
}
