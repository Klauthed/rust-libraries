//! Claim-level ID token validation: [`IdTokenValidation`] and
//! [`validate_id_token`] (no cryptography — signature checks live in
//! klauthed-security).

use crate::ProtocolError;

use super::IdTokenClaims;

/// Parameters for validating the *claims* of an OIDC ID token
/// (OpenID Connect Core 1.0 section 3.1.3.7).
///
/// This drives [`validate_id_token`], which performs only claim-level checks:
/// `iss`/`aud`/`exp`/`iat`/`nonce`. It does **not** verify the JWT signature,
/// decode the token, fetch JWKS, or perform any cryptography — that is the job
/// of `klauthed-security`. Validate the signature first, then validate claims
/// with this.
#[derive(Debug, Clone)]
pub struct IdTokenValidation {
    /// The issuer the relying party expects (must equal the token's `iss`).
    pub expected_issuer: String,

    /// The audience the relying party expects (its `client_id`); the token's
    /// `aud` must contain this value.
    pub expected_audience: String,

    /// The current time, in seconds since the Unix epoch.
    pub now: i64,

    /// If set, the token's `nonce` claim must equal this value.
    pub expected_nonce: Option<String>,

    /// Allowed clock-skew leeway, in seconds, applied to time-based checks.
    pub leeway: i64,
}

impl IdTokenValidation {
    /// Construct validation parameters with no nonce requirement and zero
    /// leeway.
    pub fn new(
        expected_issuer: impl Into<String>,
        expected_audience: impl Into<String>,
        now: i64,
    ) -> Self {
        Self {
            expected_issuer: expected_issuer.into(),
            expected_audience: expected_audience.into(),
            now,
            expected_nonce: None,
            leeway: 0,
        }
    }

    /// Require the ID token to carry a matching `nonce`.
    #[must_use]
    pub fn with_nonce(mut self, nonce: impl Into<String>) -> Self {
        self.expected_nonce = Some(nonce.into());
        self
    }

    /// Set the allowed clock-skew leeway, in seconds.
    #[must_use]
    pub fn with_leeway(mut self, leeway: i64) -> Self {
        self.leeway = leeway;
        self
    }
}

/// Validate the *claims* of an already-decoded OIDC ID token against `opts`.
///
/// Performs the claim-level checks from OIDC Core 1.0 section 3.1.3.7:
///
/// * `iss` equals the expected issuer (exactly), else
///   [`ProtocolError::IssuerMismatch`].
/// * `aud` contains the expected audience (the `client_id`), else
///   [`ProtocolError::AudienceMismatch`].
/// * `exp` is strictly after `now - leeway`, else
///   [`ProtocolError::IdTokenExpired`].
/// * `iat` is not implausibly in the future (no later than `now + leeway`),
///   else [`ProtocolError::IdTokenNotYetValid`].
/// * if `opts.expected_nonce` is set, `nonce` is present and equal, else
///   [`ProtocolError::NonceMismatch`].
///
/// # Not performed
///
/// This function does **no** cryptography: it does not verify the JWT
/// signature, check the `alg`, fetch or validate JWKS, or decode the token. Do
/// that first in `klauthed-security`; this is a pure, side-effect-free check on
/// already-parsed [`IdTokenClaims`].
pub fn validate_id_token(
    claims: &IdTokenClaims,
    opts: &IdTokenValidation,
) -> Result<(), ProtocolError> {
    // iss must match exactly.
    if claims.iss != opts.expected_issuer {
        return Err(ProtocolError::IssuerMismatch {
            expected: opts.expected_issuer.clone(),
            actual: claims.iss.clone(),
        });
    }

    // aud must contain the expected audience (client_id).
    if !claims.aud.contains(&opts.expected_audience) {
        return Err(ProtocolError::AudienceMismatch { expected: opts.expected_audience.clone() });
    }

    // exp must be strictly after (now - leeway).
    if claims.exp <= opts.now - opts.leeway {
        return Err(ProtocolError::IdTokenExpired {
            exp: claims.exp,
            now: opts.now,
            leeway: opts.leeway,
        });
    }

    // iat must not be implausibly far in the future.
    if claims.iat > opts.now + opts.leeway {
        return Err(ProtocolError::IdTokenNotYetValid {
            iat: claims.iat,
            now: opts.now,
            leeway: opts.leeway,
        });
    }

    // nonce must match when one is expected.
    if let Some(expected) = &opts.expected_nonce
        && claims.nonce.as_deref() != Some(expected.as_str())
    {
        return Err(ProtocolError::NonceMismatch);
    }

    Ok(())
}
