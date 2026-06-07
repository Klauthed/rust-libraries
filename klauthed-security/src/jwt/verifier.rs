//! [`JwtVerifier`] — validates and decodes JWTs back into [`Claims`].
//!
//! Supports HS256, RS256, ES256 (ECDSA P-256), and EdDSA (Ed25519). Asymmetric
//! public keys can be supplied as PEM or DER.

use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};

use crate::error::SecurityError;

use super::Claims;

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
        Ok(Self { key, validation: default_validation(Algorithm::RS256) })
    }

    /// An RS256 verifier from a DER-encoded RSA **public** key.
    #[must_use]
    pub fn rs256_der(public_key_der: &[u8]) -> Self {
        Self {
            key: DecodingKey::from_rsa_der(public_key_der),
            validation: default_validation(Algorithm::RS256),
        }
    }

    /// An ES256 (ECDSA P-256) verifier from an EC **public** key in PEM form.
    ///
    /// # Errors
    /// Returns [`SecurityError::Key`] if the PEM cannot be parsed.
    pub fn es256_pem(public_key_pem: &[u8]) -> Result<Self, SecurityError> {
        let key = DecodingKey::from_ec_pem(public_key_pem)
            .map_err(|e| SecurityError::Key(e.to_string()))?;
        Ok(Self { key, validation: default_validation(Algorithm::ES256) })
    }

    /// An ES256 verifier from the raw EC **public** key point (`0x04 ‖ X ‖ Y`).
    #[must_use]
    pub fn es256_der(public_key_der: &[u8]) -> Self {
        Self {
            key: DecodingKey::from_ec_der(public_key_der),
            validation: default_validation(Algorithm::ES256),
        }
    }

    /// An EdDSA (Ed25519) verifier from a **public** key in PEM form.
    ///
    /// # Errors
    /// Returns [`SecurityError::Key`] if the PEM cannot be parsed.
    pub fn eddsa_pem(public_key_pem: &[u8]) -> Result<Self, SecurityError> {
        let key = DecodingKey::from_ed_pem(public_key_pem)
            .map_err(|e| SecurityError::Key(e.to_string()))?;
        Ok(Self { key, validation: default_validation(Algorithm::EdDSA) })
    }

    /// An EdDSA verifier from the raw 32-byte Ed25519 **public** key.
    #[must_use]
    pub fn eddsa_der(public_key_der: &[u8]) -> Self {
        Self {
            key: DecodingKey::from_ed_der(public_key_der),
            validation: default_validation(Algorithm::EdDSA),
        }
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
