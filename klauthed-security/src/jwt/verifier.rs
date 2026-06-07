//! [`JwtVerifier`] — validates and decodes JWTs back into [`Claims`].

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
