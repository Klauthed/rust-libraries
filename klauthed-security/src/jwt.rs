//! JSON Web Tokens (signing + verification).
//!
//! Wraps the vetted [`jsonwebtoken`] crate with a klauthed-flavoured API:
//!
//! * [`Claims`] — the standard registered claims plus a bag of custom claims.
//! * [`JwtSigner`] — encodes [`Claims`] into a compact JWT.
//! * [`JwtVerifier`] — decodes + validates a JWT back into [`Claims`].
//!
//! Both HS256 (a shared secret) and RS256 (an RSA key pair, PEM-encoded) are
//! supported. Expiry is computed from a [`Clock`] so it stays testable.
//!
//! ```
//! use klauthed_security::jwt::{Claims, JwtSigner, JwtVerifier};
//! use klauthed_core::time::SystemClock;
//! use chrono::Duration;
//!
//! let signer = JwtSigner::hs256(b"super-secret-signing-key");
//! let verifier = JwtVerifier::hs256(b"super-secret-signing-key");
//!
//! let claims = Claims::builder("user-123", &SystemClock, Duration::minutes(15))
//!     .issuer("klauthed")
//!     .audience("klauthed-api")
//!     .build();
//!
//! let token = signer.encode(&claims).unwrap();
//! let decoded = verifier
//!     .expecting_issuer("klauthed")
//!     .expecting_audience("klauthed-api")
//!     .decode(&token)
//!     .unwrap();
//! assert_eq!(decoded.sub.as_deref(), Some("user-123"));
//! ```

use jsonwebtoken::{
    decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation,
};
use serde::{Deserialize, Serialize};

use klauthed_core::time::Clock;

use crate::error::SecurityError;
use crate::token::random_token;

/// JWT claims: the standard registered claims plus arbitrary custom claims.
///
/// All registered claims are optional so the struct can model both minted and
/// decoded tokens. Custom claims are flattened into the top-level JSON object,
/// so `claims.custom.insert("role", json!("admin"))` emits `"role":"admin"`
/// alongside `sub`, `exp`, etc.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claims {
    /// Subject — who the token is about (e.g. a user id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub: Option<String>,
    /// Issuer — who minted the token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iss: Option<String>,
    /// Audience — who the token is intended for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aud: Option<String>,
    /// Expiration time (seconds since the Unix epoch).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exp: Option<i64>,
    /// Issued-at time (seconds since the Unix epoch).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iat: Option<i64>,
    /// Not-before time (seconds since the Unix epoch).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nbf: Option<i64>,
    /// JWT id — a unique identifier for this token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jti: Option<String>,
    /// Any additional, application-specific claims.
    #[serde(flatten)]
    pub custom: serde_json::Map<String, serde_json::Value>,
}

impl Claims {
    /// Start building claims for `subject`, with `exp` = now + `ttl` derived from
    /// `clock`. `iat` and `nbf` are set to now. See [`ClaimsBuilder`].
    pub fn builder<C: Clock + ?Sized>(
        subject: impl Into<String>,
        clock: &C,
        ttl: chrono::Duration,
    ) -> ClaimsBuilder {
        let now = clock.now().into_datetime().timestamp();
        ClaimsBuilder {
            claims: Claims {
                sub: Some(subject.into()),
                iss: None,
                aud: None,
                exp: now.checked_add(ttl.num_seconds()),
                iat: Some(now),
                nbf: Some(now),
                jti: None,
                custom: serde_json::Map::new(),
            },
        }
    }
}

/// A fluent builder for [`Claims`] (see [`Claims::builder`]).
#[derive(Debug, Clone)]
pub struct ClaimsBuilder {
    claims: Claims,
}

impl ClaimsBuilder {
    /// Set the issuer (`iss`).
    #[must_use]
    pub fn issuer(mut self, iss: impl Into<String>) -> Self {
        self.claims.iss = Some(iss.into());
        self
    }

    /// Set the audience (`aud`).
    #[must_use]
    pub fn audience(mut self, aud: impl Into<String>) -> Self {
        self.claims.aud = Some(aud.into());
        self
    }

    /// Set the JWT id (`jti`).
    #[must_use]
    pub fn jwt_id(mut self, jti: impl Into<String>) -> Self {
        self.claims.jti = Some(jti.into());
        self
    }

    /// Set a fresh random JWT id (`jti`), 16 bytes of entropy, url-safe base64.
    ///
    /// # Errors
    /// Returns [`SecurityError::Rng`] if the OS CSPRNG fails.
    pub fn random_jwt_id(mut self) -> Result<Self, SecurityError> {
        self.claims.jti = Some(random_token(16)?);
        Ok(self)
    }

    /// Insert a custom claim.
    #[must_use]
    pub fn claim(mut self, key: impl Into<String>, value: impl Into<serde_json::Value>) -> Self {
        self.claims.custom.insert(key.into(), value.into());
        self
    }

    /// Finish building.
    #[must_use]
    pub fn build(self) -> Claims {
        self.claims
    }
}

/// Signs [`Claims`] into compact JWTs.
pub struct JwtSigner {
    header: Header,
    key: EncodingKey,
}

impl JwtSigner {
    /// An HS256 signer using `secret` as the shared HMAC key.
    #[must_use]
    pub fn hs256(secret: &[u8]) -> Self {
        Self {
            header: Header::new(Algorithm::HS256),
            key: EncodingKey::from_secret(secret),
        }
    }

    /// An RS256 signer from a PKCS#1/PKCS#8 RSA **private** key in PEM form.
    ///
    /// # Errors
    /// Returns [`SecurityError::Key`] if the PEM cannot be parsed.
    pub fn rs256_pem(private_key_pem: &[u8]) -> Result<Self, SecurityError> {
        let key = EncodingKey::from_rsa_pem(private_key_pem)
            .map_err(|e| SecurityError::Key(e.to_string()))?;
        Ok(Self {
            header: Header::new(Algorithm::RS256),
            key,
        })
    }

    /// Encode and sign `claims`, returning the compact JWT string.
    ///
    /// # Errors
    /// Returns [`SecurityError::Encode`] if serialization/signing fails.
    pub fn encode(&self, claims: &Claims) -> Result<String, SecurityError> {
        encode(&self.header, claims, &self.key).map_err(|e| SecurityError::Encode(e.to_string()))
    }
}

/// Verifies and decodes JWTs back into [`Claims`].
pub struct JwtVerifier {
    key: DecodingKey,
    validation: Validation,
}

impl JwtVerifier {
    /// An HS256 verifier using `secret` as the shared HMAC key.
    ///
    /// By default `exp` and `nbf` are validated. Use [`expecting_issuer`] /
    /// [`expecting_audience`] to additionally enforce `iss` / `aud`.
    ///
    /// [`expecting_issuer`]: JwtVerifier::expecting_issuer
    /// [`expecting_audience`]: JwtVerifier::expecting_audience
    #[must_use]
    pub fn hs256(secret: &[u8]) -> Self {
        Self {
            key: DecodingKey::from_secret(secret),
            validation: default_validation(Algorithm::HS256),
        }
    }

    /// An RS256 verifier from an RSA **public** key in PEM form.
    ///
    /// # Errors
    /// Returns [`SecurityError::Key`] if the PEM cannot be parsed.
    pub fn rs256_pem(public_key_pem: &[u8]) -> Result<Self, SecurityError> {
        let key = DecodingKey::from_rsa_pem(public_key_pem)
            .map_err(|e| SecurityError::Key(e.to_string()))?;
        Ok(Self {
            key,
            validation: default_validation(Algorithm::RS256),
        })
    }

    /// Require the token's `iss` to equal `issuer`.
    #[must_use]
    pub fn expecting_issuer(mut self, issuer: impl Into<String>) -> Self {
        self.validation.set_issuer(&[issuer.into()]);
        self
    }

    /// Require the token's `aud` to equal `audience`.
    #[must_use]
    pub fn expecting_audience(mut self, audience: impl Into<String>) -> Self {
        self.validation.set_audience(&[audience.into()]);
        self
    }

    /// Allow `leeway` seconds of clock skew when checking `exp`/`nbf`.
    #[must_use]
    pub fn leeway_seconds(mut self, leeway: u64) -> Self {
        self.validation.leeway = leeway;
        self
    }

    /// Verify the signature and registered-claim constraints of `token`, then
    /// decode its [`Claims`].
    ///
    /// # Errors
    /// * [`SecurityError::ExpiredToken`] — the token's `exp` has passed.
    /// * [`SecurityError::InvalidToken`] — bad signature, or `iss`/`aud`/`nbf`
    ///   mismatch.
    /// * [`SecurityError::MalformedToken`] — the token isn't a well-formed JWT.
    pub fn decode(&self, token: &str) -> Result<Claims, SecurityError> {
        decode::<Claims>(token, &self.key, &self.validation)
            .map(|data| data.claims)
            .map_err(map_jwt_error)
    }
}

/// Default validation: validate `exp` and `nbf`, accept the given algorithm.
fn default_validation(alg: Algorithm) -> Validation {
    let mut v = Validation::new(alg);
    v.validate_exp = true;
    v.validate_nbf = true;
    v
}

/// Translate a `jsonwebtoken` error into the crate's error taxonomy, keeping the
/// expired-vs-invalid distinction the callers care about.
fn map_jwt_error(err: jsonwebtoken::errors::Error) -> SecurityError {
    use jsonwebtoken::errors::ErrorKind;
    match err.kind() {
        ErrorKind::ExpiredSignature => SecurityError::ExpiredToken,
        ErrorKind::InvalidSignature
        | ErrorKind::InvalidIssuer
        | ErrorKind::InvalidAudience
        | ErrorKind::InvalidSubject
        | ErrorKind::ImmatureSignature => SecurityError::InvalidToken(err.to_string()),
        _ => SecurityError::MalformedToken(err.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use klauthed_core::time::{FixedClock, SystemClock, Timestamp};

    /// A clock pinned to "now", so a token minted with a positive TTL is still
    /// valid when the verifier checks it against the real wall clock.
    fn now_clock() -> FixedClock {
        FixedClock::new(Timestamp::now())
    }

    #[test]
    fn hs256_round_trip() {
        let signer = JwtSigner::hs256(b"shared-secret");
        let verifier = JwtVerifier::hs256(b"shared-secret");

        let claims = Claims::builder("user-1", &now_clock(), chrono::Duration::hours(1))
            .issuer("klauthed")
            .audience("api")
            .claim("role", "admin")
            .build();

        let token = signer.encode(&claims).unwrap();
        let decoded = verifier
            .expecting_issuer("klauthed")
            .expecting_audience("api")
            .decode(&token)
            .unwrap();

        assert_eq!(decoded.sub.as_deref(), Some("user-1"));
        assert_eq!(decoded.iss.as_deref(), Some("klauthed"));
        assert_eq!(
            decoded.custom.get("role").and_then(|v| v.as_str()),
            Some("admin")
        );
    }

    #[test]
    fn wrong_secret_is_invalid_token() {
        let token = JwtSigner::hs256(b"key-a")
            .encode(&Claims::builder("u", &now_clock(), chrono::Duration::hours(1)).build())
            .unwrap();
        let err = JwtVerifier::hs256(b"key-b").decode(&token).unwrap_err();
        assert!(matches!(err, SecurityError::InvalidToken(_)));
    }

    #[test]
    fn expired_token_is_detected() {
        // exp one hour in the past relative to the signing clock.
        let token = JwtSigner::hs256(b"k")
            .encode(&Claims::builder("u", &now_clock(), chrono::Duration::hours(-1)).build())
            .unwrap();
        // Verifier uses the real wall clock, so the past `exp` is expired.
        let err = JwtVerifier::hs256(b"k").decode(&token).unwrap_err();
        assert!(matches!(err, SecurityError::ExpiredToken));
    }

    #[test]
    fn wrong_issuer_is_invalid() {
        let token = JwtSigner::hs256(b"k")
            .encode(
                &Claims::builder("u", &now_clock(), chrono::Duration::hours(1))
                    .issuer("real")
                    .build(),
            )
            .unwrap();
        let err = JwtVerifier::hs256(b"k")
            .expecting_issuer("expected")
            .decode(&token)
            .unwrap_err();
        assert!(matches!(err, SecurityError::InvalidToken(_)));
    }

    #[test]
    fn malformed_token_is_detected() {
        let err = JwtVerifier::hs256(b"k").decode("not.a.jwt").unwrap_err();
        assert!(matches!(err, SecurityError::MalformedToken(_)));
    }

    #[test]
    fn round_trips_with_system_clock() {
        let signer = JwtSigner::hs256(b"k");
        let token = signer
            .encode(&Claims::builder("sys", &SystemClock, chrono::Duration::minutes(5)).build())
            .unwrap();
        let decoded = JwtVerifier::hs256(b"k").decode(&token).unwrap();
        assert_eq!(decoded.sub.as_deref(), Some("sys"));
        assert!(decoded.exp.is_some() && decoded.iat.is_some());
    }

    #[test]
    fn random_jwt_id_is_set() {
        let claims = Claims::builder("u", &now_clock(), chrono::Duration::hours(1))
            .random_jwt_id()
            .unwrap()
            .build();
        assert!(claims.jti.is_some());
    }
}
