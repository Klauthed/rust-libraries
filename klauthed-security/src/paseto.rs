//! PASETO **v4** tokens (`paseto` feature).
//!
//! PASETO is a misuse-resistant alternative to JWT: the version + purpose are
//! baked into the token, so there is no algorithm field to confuse and no
//! `alg=none` class of bug. Two purposes are supported:
//!
//! * [`PasetoV4Signer`] / [`PasetoV4Verifier`] — **v4.public**, Ed25519-signed.
//!   Anyone with the public key can verify; only the secret-key holder can mint.
//!   The claims are readable (signed, not encrypted).
//! * [`PasetoV4Local`] — **v4.local**, XChaCha20-Poly1305 symmetric encryption.
//!   The claims are confidential; the same key mints and reads.
//!
//! Tokens carry the same [`Claims`] as [`crate::jwt`], so a service can issue
//! any of these formats from one claim set. PASETO encodes the time claims
//! (`exp`/`nbf`/`iat`) as RFC 3339 strings (not JWT's numeric dates); the
//! conversion is handled here.
//!
//! ```
//! use klauthed_core::time::{Duration, SystemClock};
//! use klauthed_security::{Claims, paseto::PasetoV4Signer};
//!
//! # fn main() -> Result<(), klauthed_security::SecurityError> {
//! let (signer, verifier) = PasetoV4Signer::generate()?;
//! let claims = Claims::builder("user-123", &SystemClock, Duration::hours(1))
//!     .issuer("klauthed")
//!     .build();
//!
//! let token = signer.encode(&claims)?;
//! let decoded = verifier.decode(&token)?;
//! assert_eq!(decoded.sub.as_deref(), Some("user-123"));
//! # Ok(())
//! # }
//! ```

use pasetors::claims::{Claims as PasetoClaims, ClaimsValidationRules};
use pasetors::errors::Error as PasetoError;
use pasetors::keys::{
    AsymmetricKeyPair, AsymmetricPublicKey, AsymmetricSecretKey, Generate, SymmetricKey,
};
use pasetors::token::UntrustedToken;
use pasetors::version4::V4;
use pasetors::{Local, Public, local, public};
use ring::rand::{SecureRandom, SystemRandom};

use klauthed_core::time::Timestamp;

use crate::error::SecurityError;
use crate::jwt::Claims;

/// A PASETO **v4.public** signer (Ed25519). Mints `v4.public.…` tokens from
/// [`Claims`]; share the paired [`PasetoV4Verifier`] for verification.
pub struct PasetoV4Signer {
    secret: AsymmetricSecretKey<V4>,
}

/// A PASETO **v4.public** verifier (Ed25519). Verifies the signature and the
/// `exp`/`nbf` time claims; optionally enforces `iss` / `aud`.
pub struct PasetoV4Verifier {
    public: AsymmetricPublicKey<V4>,
    issuer: Option<String>,
    audience: Option<String>,
}

impl PasetoV4Signer {
    /// Generate a fresh Ed25519 keypair and return the matched signer + verifier.
    ///
    /// # Errors
    /// Returns [`SecurityError::Rng`] if the OS CSPRNG fails.
    pub fn generate() -> Result<(PasetoV4Signer, PasetoV4Verifier), SecurityError> {
        let keypair = AsymmetricKeyPair::<V4>::generate().map_err(|_| SecurityError::Rng)?;
        Ok((
            PasetoV4Signer { secret: keypair.secret },
            PasetoV4Verifier { public: keypair.public, issuer: None, audience: None },
        ))
    }

    /// Load a signer from a PASETO v4 Ed25519 secret key (raw bytes).
    ///
    /// # Errors
    /// Returns [`SecurityError::Key`] if the bytes are not a valid v4 secret key.
    pub fn from_secret_key(bytes: &[u8]) -> Result<Self, SecurityError> {
        let secret = AsymmetricSecretKey::<V4>::from(bytes)
            .map_err(|e| SecurityError::Key(e.to_string()))?;
        Ok(Self { secret })
    }

    /// Sign `claims` into a `v4.public.…` token string.
    ///
    /// # Errors
    /// Returns [`SecurityError::Encode`] if the claims cannot be assembled or signed.
    pub fn encode(&self, claims: &Claims) -> Result<String, SecurityError> {
        let payload = to_paseto_claims(claims)?;
        public::sign(&self.secret, &payload, None, None)
            .map_err(|e| SecurityError::Encode(e.to_string()))
    }
}

impl PasetoV4Verifier {
    /// Load a verifier from a PASETO v4 Ed25519 public key (raw bytes).
    ///
    /// # Errors
    /// Returns [`SecurityError::Key`] if the bytes are not a valid v4 public key.
    pub fn from_public_key(bytes: &[u8]) -> Result<Self, SecurityError> {
        let public = AsymmetricPublicKey::<V4>::from(bytes)
            .map_err(|e| SecurityError::Key(e.to_string()))?;
        Ok(Self { public, issuer: None, audience: None })
    }

    /// Additionally require the token's issuer (`iss`) to equal `iss`.
    #[must_use]
    pub fn expecting_issuer(mut self, iss: impl Into<String>) -> Self {
        self.issuer = Some(iss.into());
        self
    }

    /// Additionally require the token's audience (`aud`) to equal `aud`.
    #[must_use]
    pub fn expecting_audience(mut self, aud: impl Into<String>) -> Self {
        self.audience = Some(aud.into());
        self
    }

    /// Verify a `v4.public` token and return its [`Claims`].
    ///
    /// Checks the Ed25519 signature and the `exp` / `nbf` time claims (and
    /// `iss` / `aud` when configured).
    ///
    /// # Errors
    /// * [`SecurityError::MalformedToken`] — not a parseable `v4.public` token.
    /// * [`SecurityError::InvalidToken`] — bad signature, expired/not-yet-valid,
    ///   or a failed issuer/audience check.
    pub fn decode(&self, token: &str) -> Result<Claims, SecurityError> {
        let untrusted = UntrustedToken::<Public, V4>::try_from(token)
            .map_err(|e| SecurityError::MalformedToken(e.to_string()))?;

        let mut rules = ClaimsValidationRules::new();
        if let Some(iss) = &self.issuer {
            rules.validate_issuer_with(iss);
        }
        if let Some(aud) = &self.audience {
            rules.validate_audience_with(aud);
        }

        let trusted = public::verify(&self.public, &untrusted, &rules, None, None)
            .map_err(|e| SecurityError::InvalidToken(e.to_string()))?;
        let payload = trusted
            .payload_claims()
            .ok_or_else(|| SecurityError::InvalidToken("token carried no claims".to_owned()))?;
        from_paseto_claims(payload)
    }
}

/// A PASETO **v4.local** cipher (XChaCha20-Poly1305). Encrypts [`Claims`] into an
/// opaque, tamper-evident `v4.local.…` token under a symmetric key — the same key
/// mints and reads it, and the payload stays confidential (unlike the signed,
/// readable [`PasetoV4Signer`] tokens).
pub struct PasetoV4Local {
    key: SymmetricKey<V4>,
    issuer: Option<String>,
    audience: Option<String>,
}

impl PasetoV4Local {
    /// Generate a fresh random 32-byte key.
    ///
    /// # Errors
    /// Returns [`SecurityError::Rng`] if the OS CSPRNG fails, or
    /// [`SecurityError::Key`] if the key is rejected.
    pub fn generate() -> Result<Self, SecurityError> {
        let mut bytes = [0u8; 32];
        SystemRandom::new().fill(&mut bytes).map_err(|_| SecurityError::Rng)?;
        Self::from_key(&bytes)
    }

    /// Load from a 32-byte symmetric key.
    ///
    /// # Errors
    /// Returns [`SecurityError::Key`] if the bytes are not a valid v4 key.
    pub fn from_key(bytes: &[u8]) -> Result<Self, SecurityError> {
        let key = SymmetricKey::<V4>::from(bytes).map_err(|e| SecurityError::Key(e.to_string()))?;
        Ok(Self { key, issuer: None, audience: None })
    }

    /// Additionally require the token's issuer (`iss`) on [`decode`](Self::decode).
    #[must_use]
    pub fn expecting_issuer(mut self, iss: impl Into<String>) -> Self {
        self.issuer = Some(iss.into());
        self
    }

    /// Additionally require the token's audience (`aud`) on [`decode`](Self::decode).
    #[must_use]
    pub fn expecting_audience(mut self, aud: impl Into<String>) -> Self {
        self.audience = Some(aud.into());
        self
    }

    /// Encrypt `claims` into a `v4.local.…` token string.
    ///
    /// # Errors
    /// Returns [`SecurityError::Encode`] if the claims cannot be assembled or encrypted.
    pub fn encode(&self, claims: &Claims) -> Result<String, SecurityError> {
        let payload = to_paseto_claims(claims)?;
        local::encrypt(&self.key, &payload, None, None)
            .map_err(|e| SecurityError::Encode(e.to_string()))
    }

    /// Decrypt and validate a `v4.local.…` token, returning its [`Claims`].
    ///
    /// Checks the authentication tag and the `exp` / `nbf` time claims (and
    /// `iss` / `aud` when configured).
    ///
    /// # Errors
    /// * [`SecurityError::MalformedToken`] — not a parseable `v4.local` token.
    /// * [`SecurityError::InvalidToken`] — wrong key / tampered, expired/not-yet-valid,
    ///   or a failed issuer/audience check.
    pub fn decode(&self, token: &str) -> Result<Claims, SecurityError> {
        let untrusted = UntrustedToken::<Local, V4>::try_from(token)
            .map_err(|e| SecurityError::MalformedToken(e.to_string()))?;

        let mut rules = ClaimsValidationRules::new();
        if let Some(iss) = &self.issuer {
            rules.validate_issuer_with(iss);
        }
        if let Some(aud) = &self.audience {
            rules.validate_audience_with(aud);
        }

        let trusted = local::decrypt(&self.key, &untrusted, &rules, None, None)
            .map_err(|e| SecurityError::InvalidToken(e.to_string()))?;
        let payload = trusted
            .payload_claims()
            .ok_or_else(|| SecurityError::InvalidToken("token carried no claims".to_owned()))?;
        from_paseto_claims(payload)
    }
}

/// Map klauthed [`Claims`] onto pasetors claims, rendering the numeric time
/// claims as the RFC 3339 strings PASETO requires.
fn to_paseto_claims(claims: &Claims) -> Result<PasetoClaims, SecurityError> {
    let enc = |e: PasetoError| SecurityError::Encode(format!("{e:?}"));
    let rfc = |secs: i64| Timestamp::from_unix_seconds(secs).to_rfc3339();

    let mut out = PasetoClaims::new().map_err(enc)?;
    match claims.exp {
        Some(exp) => out.expiration(&rfc(exp)).map_err(enc)?,
        None => out.non_expiring(),
    }
    if let Some(iat) = claims.iat {
        out.issued_at(&rfc(iat)).map_err(enc)?;
    }
    if let Some(nbf) = claims.nbf {
        out.not_before(&rfc(nbf)).map_err(enc)?;
    }
    if let Some(sub) = &claims.sub {
        out.subject(sub).map_err(enc)?;
    }
    if let Some(iss) = &claims.iss {
        out.issuer(iss).map_err(enc)?;
    }
    if let Some(aud) = &claims.aud {
        out.audience(aud).map_err(enc)?;
    }
    if let Some(jti) = &claims.jti {
        out.token_identifier(jti).map_err(enc)?;
    }
    for (key, value) in &claims.custom {
        out.add_additional(key, value.clone()).map_err(enc)?;
    }
    Ok(out)
}

/// Map verified pasetors claims back onto klauthed [`Claims`], parsing the
/// RFC 3339 time claims back to Unix seconds.
fn from_paseto_claims(payload: &PasetoClaims) -> Result<Claims, SecurityError> {
    let invalid = |e: PasetoError| SecurityError::InvalidToken(format!("{e:?}"));
    let json = payload.to_string().map_err(invalid)?;
    let value: serde_json::Value = serde_json::from_str(&json)
        .map_err(|e| SecurityError::InvalidToken(format!("claims JSON: {e}")))?;
    let object = value
        .as_object()
        .ok_or_else(|| SecurityError::InvalidToken("claims were not a JSON object".to_owned()))?;

    let take_str =
        |key: &str| object.get(key).and_then(serde_json::Value::as_str).map(str::to_owned);
    let take_secs = |key: &str| {
        object
            .get(key)
            .and_then(serde_json::Value::as_str)
            .and_then(Timestamp::parse_rfc3339)
            .map(|ts| ts.unix_seconds())
    };

    const REGISTERED: [&str; 7] = ["sub", "iss", "aud", "exp", "iat", "nbf", "jti"];
    let custom = object
        .iter()
        .filter(|(key, _)| !REGISTERED.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();

    Ok(Claims {
        sub: take_str("sub"),
        iss: take_str("iss"),
        aud: take_str("aud"),
        exp: take_secs("exp"),
        iat: take_secs("iat"),
        nbf: take_secs("nbf"),
        jti: take_str("jti"),
        custom,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use klauthed_core::time::{Duration, FixedClock, SystemClock};
    use serde_json::json;

    fn claims() -> Claims {
        Claims::builder("user-123", &SystemClock, Duration::hours(1))
            .issuer("klauthed")
            .audience("api")
            .claim("role", json!("admin"))
            .build()
    }

    #[test]
    fn round_trips_claims() {
        let (signer, verifier) = PasetoV4Signer::generate().unwrap();
        let token = signer.encode(&claims()).unwrap();
        assert!(token.starts_with("v4.public."), "{token}");

        let decoded = verifier.decode(&token).unwrap();
        assert_eq!(decoded.sub.as_deref(), Some("user-123"));
        assert_eq!(decoded.iss.as_deref(), Some("klauthed"));
        assert_eq!(decoded.custom.get("role"), Some(&json!("admin")));
        assert!(decoded.exp.is_some());
    }

    #[test]
    fn rejects_a_token_signed_by_a_different_key() {
        let (signer, _) = PasetoV4Signer::generate().unwrap();
        let (_, other_verifier) = PasetoV4Signer::generate().unwrap();
        let token = signer.encode(&claims()).unwrap();

        let err = other_verifier.decode(&token).unwrap_err();
        assert!(matches!(err, SecurityError::InvalidToken(_)), "{err:?}");
    }

    #[test]
    fn rejects_an_expired_token() {
        let (signer, verifier) = PasetoV4Signer::generate().unwrap();
        // A clock pinned to 2023 with a 1h TTL ⇒ exp is far in the past.
        let past = FixedClock::at_unix_millis(1_700_000_000_000);
        let expired = Claims::builder("user-123", &past, Duration::hours(1)).build();
        let token = signer.encode(&expired).unwrap();

        let err = verifier.decode(&token).unwrap_err();
        assert!(matches!(err, SecurityError::InvalidToken(_)), "{err:?}");
    }

    #[test]
    fn rejects_a_mismatched_issuer() {
        let (signer, verifier) = PasetoV4Signer::generate().unwrap();
        let token = signer.encode(&claims()).unwrap();

        let err = verifier.expecting_issuer("someone-else").decode(&token).unwrap_err();
        assert!(matches!(err, SecurityError::InvalidToken(_)), "{err:?}");
    }

    #[test]
    fn rejects_malformed_input() {
        let (_, verifier) = PasetoV4Signer::generate().unwrap();
        let err = verifier.decode("not-a-paseto-token").unwrap_err();
        assert!(matches!(err, SecurityError::MalformedToken(_)), "{err:?}");
    }

    // ── v4.local ──────────────────────────────────────────────────────────────

    #[test]
    fn local_round_trips_claims() {
        let cipher = PasetoV4Local::generate().unwrap();
        let token = cipher.encode(&claims()).unwrap();
        assert!(token.starts_with("v4.local."), "{token}");

        let decoded = cipher.decode(&token).unwrap();
        assert_eq!(decoded.sub.as_deref(), Some("user-123"));
        assert_eq!(decoded.custom.get("role"), Some(&json!("admin")));
    }

    #[test]
    fn local_rejects_a_token_under_a_different_key() {
        let cipher = PasetoV4Local::generate().unwrap();
        let other = PasetoV4Local::generate().unwrap();
        let token = cipher.encode(&claims()).unwrap();

        let err = other.decode(&token).unwrap_err();
        assert!(matches!(err, SecurityError::InvalidToken(_)), "{err:?}");
    }

    #[test]
    fn local_rejects_an_expired_token() {
        let cipher = PasetoV4Local::generate().unwrap();
        let past = FixedClock::at_unix_millis(1_700_000_000_000);
        let expired = Claims::builder("user-123", &past, Duration::hours(1)).build();
        let token = cipher.encode(&expired).unwrap();

        let err = cipher.decode(&token).unwrap_err();
        assert!(matches!(err, SecurityError::InvalidToken(_)), "{err:?}");
    }

    #[test]
    fn local_rejects_malformed_input() {
        let cipher = PasetoV4Local::generate().unwrap();
        let err = cipher.decode("not-a-paseto-token").unwrap_err();
        assert!(matches!(err, SecurityError::MalformedToken(_)), "{err:?}");
    }
}
