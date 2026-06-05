//! OpenID Connect (OIDC) data types.
//!
//! Spec-accurate serde models for the OIDC discovery document, the standard
//! claim set, and ID token claims. These are pure data types: no OAuth flows,
//! no token validation crypto, no HTTP. JWT signing/verification lives in
//! `klauthed-security`.
//!
//! Field names match the JSON wire format exactly. OIDC uses snake_case, which
//! already matches Rust idiom, so most fields need no rename. Optional fields
//! are `Option`/`Vec` and skipped on serialization when empty.
//!
//! References:
//! * OpenID Connect Discovery 1.0
//! * OpenID Connect Core 1.0 (sections 2 and 5.1)
//!
//! ```
//! use klauthed_protocol::oidc::{ProviderMetadata, ResponseType, SubjectType};
//!
//! let meta = ProviderMetadata {
//!     issuer: "https://issuer.example.com".into(),
//!     authorization_endpoint: Some("https://issuer.example.com/authorize".into()),
//!     token_endpoint: Some("https://issuer.example.com/token".into()),
//!     jwks_uri: Some("https://issuer.example.com/jwks".into()),
//!     response_types_supported: vec![ResponseType::Code],
//!     subject_types_supported: vec![SubjectType::Public],
//!     id_token_signing_alg_values_supported: vec!["RS256".into()],
//!     ..Default::default()
//! };
//! let json = serde_json::to_value(&meta).unwrap();
//! assert_eq!(json["issuer"], "https://issuer.example.com");
//! assert_eq!(json["response_types_supported"][0], "code");
//! ```

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ProtocolError;

/// The `response_type` values an authorization server supports.
///
/// Serializes to the space-delimited string forms from OAuth 2.0 / OIDC Core,
/// e.g. `"code"`, `"id_token"`, `"code id_token"`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ResponseType {
    /// Authorization Code flow.
    #[default]
    #[serde(rename = "code")]
    Code,
    /// Implicit flow returning only an ID token.
    #[serde(rename = "id_token")]
    IdToken,
    /// Implicit flow returning an ID token and an access token.
    #[serde(rename = "id_token token")]
    IdTokenToken,
    /// Hybrid flow.
    #[serde(rename = "code id_token")]
    CodeIdToken,
    /// Hybrid flow.
    #[serde(rename = "code token")]
    CodeToken,
    /// Hybrid flow.
    #[serde(rename = "code id_token token")]
    CodeIdTokenToken,
    /// The `none` response type (no tokens; used for `response_type=none`).
    #[serde(rename = "none")]
    None,
}

/// The OAuth 2.0 `grant_type` values an authorization server supports.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum GrantType {
    /// `authorization_code`.
    #[default]
    #[serde(rename = "authorization_code")]
    AuthorizationCode,
    /// `implicit`.
    #[serde(rename = "implicit")]
    Implicit,
    /// `refresh_token`.
    #[serde(rename = "refresh_token")]
    RefreshToken,
    /// `client_credentials`.
    #[serde(rename = "client_credentials")]
    ClientCredentials,
    /// `password` (Resource Owner Password Credentials).
    #[serde(rename = "password")]
    Password,
    /// `urn:ietf:params:oauth:grant-type:device_code`.
    #[serde(rename = "urn:ietf:params:oauth:grant-type:device_code")]
    DeviceCode,
    /// `urn:ietf:params:oauth:grant-type:jwt-bearer`.
    #[serde(rename = "urn:ietf:params:oauth:grant-type:jwt-bearer")]
    JwtBearer,
    /// `urn:ietf:params:oauth:grant-type:token-exchange`.
    #[serde(rename = "urn:ietf:params:oauth:grant-type:token-exchange")]
    TokenExchange,
}

/// How the authorization server computes the `sub` (subject) claim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum SubjectType {
    /// The same `sub` value is returned to all clients.
    #[serde(rename = "public")]
    Public,
    /// A different `sub` value is returned to each client.
    #[serde(rename = "pairwise")]
    Pairwise,
}

/// Commonly requested OIDC scopes, plus an escape hatch for custom values.
///
/// Serializes to the bare scope token (`"openid"`, `"email"`, …).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Scope {
    /// One of the well-known OIDC scopes.
    Known(KnownScope),
    /// Any other scope token.
    Other(String),
}

/// The standard OIDC scopes from Core 1.0 section 5.4.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum KnownScope {
    /// `openid` — required to invoke OIDC behavior.
    #[serde(rename = "openid")]
    OpenId,
    /// `profile` — profile claims (`name`, `picture`, …).
    #[serde(rename = "profile")]
    Profile,
    /// `email` — `email` and `email_verified`.
    #[serde(rename = "email")]
    Email,
    /// `address` — the `address` claim.
    #[serde(rename = "address")]
    Address,
    /// `phone` — `phone_number` and `phone_number_verified`.
    #[serde(rename = "phone")]
    Phone,
    /// `offline_access` — request a refresh token.
    #[serde(rename = "offline_access")]
    OfflineAccess,
}

impl Scope {
    /// The bare scope token (`"openid"`, a custom string, …).
    pub fn as_str(&self) -> &str {
        match self {
            Scope::Known(k) => match k {
                KnownScope::OpenId => "openid",
                KnownScope::Profile => "profile",
                KnownScope::Email => "email",
                KnownScope::Address => "address",
                KnownScope::Phone => "phone",
                KnownScope::OfflineAccess => "offline_access",
            },
            Scope::Other(s) => s,
        }
    }
}

/// The OIDC discovery document served at
/// `/.well-known/openid-configuration` (OpenID Connect Discovery 1.0).
///
/// `issuer` is the only required field; everything else is optional and skipped
/// when absent/empty so a serialized document matches what a server actually
/// advertises.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderMetadata {
    /// REQUIRED. The issuer identifier URL.
    pub issuer: String,

    /// URL of the authorization endpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorization_endpoint: Option<String>,

    /// URL of the token endpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_endpoint: Option<String>,

    /// URL of the UserInfo endpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub userinfo_endpoint: Option<String>,

    /// URL of the JSON Web Key Set document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jwks_uri: Option<String>,

    /// URL of the dynamic client registration endpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registration_endpoint: Option<String>,

    /// URL of the end-session (RP-initiated logout) endpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_session_endpoint: Option<String>,

    /// REQUIRED in the spec. The supported `response_type` values.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub response_types_supported: Vec<ResponseType>,

    /// Supported `response_mode` values (`query`, `fragment`, `form_post`, …).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub response_modes_supported: Vec<String>,

    /// Supported OAuth 2.0 grant types.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grant_types_supported: Vec<GrantType>,

    /// REQUIRED in the spec. Supported subject identifier types.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subject_types_supported: Vec<SubjectType>,

    /// REQUIRED in the spec. JWS `alg` values supported for the ID token
    /// signature (e.g. `RS256`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub id_token_signing_alg_values_supported: Vec<String>,

    /// JWS `alg` values supported for UserInfo responses signed as a JWT.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub userinfo_signing_alg_values_supported: Vec<String>,

    /// JWS `alg` values supported for request objects.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub request_object_signing_alg_values_supported: Vec<String>,

    /// Supported OAuth 2.0 scope values.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes_supported: Vec<Scope>,

    /// Claim names the provider may supply.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub claims_supported: Vec<String>,

    /// Client authentication methods supported at the token endpoint.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub token_endpoint_auth_methods_supported: Vec<String>,

    /// PKCE code challenge methods supported (`S256`, `plain`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub code_challenge_methods_supported: Vec<String>,

    /// Supported Authentication Context Class References.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub acr_values_supported: Vec<String>,

    /// URL of a human-readable service documentation page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_documentation: Option<String>,

    /// Any additional provider metadata members not modeled above.
    #[serde(flatten)]
    pub additional: BTreeMap<String, Value>,
}

/// The OIDC standard claim set (OpenID Connect Core 1.0 section 5.1).
///
/// Every field is optional; a UserInfo response or ID token only carries the
/// claims the authorization server chose to release. Field names match the
/// spec exactly.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StandardClaims {
    /// Subject identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub: Option<String>,

    /// Full name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Given (first) name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub given_name: Option<String>,

    /// Family (last) name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family_name: Option<String>,

    /// Middle name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub middle_name: Option<String>,

    /// Casual name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,

    /// Preferred username.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred_username: Option<String>,

    /// Profile page URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,

    /// Profile picture URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub picture: Option<String>,

    /// Web page or blog URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub website: Option<String>,

    /// Preferred email address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,

    /// Whether the email address has been verified.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email_verified: Option<bool>,

    /// Gender.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gender: Option<String>,

    /// Birthday, `YYYY-MM-DD` or `YYYY` form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub birthdate: Option<String>,

    /// IANA time zone (e.g. `Europe/Berlin`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zoneinfo: Option<String>,

    /// BCP47 language tag (e.g. `en-US`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,

    /// Preferred phone number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phone_number: Option<String>,

    /// Whether the phone number has been verified.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phone_number_verified: Option<bool>,

    /// Preferred postal address (a structured `address` claim).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<AddressClaim>,

    /// Time the information was last updated, seconds since the Unix epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
}

/// The OIDC structured `address` claim (Core 1.0 section 5.1.1).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddressClaim {
    /// Full mailing address, formatted for display.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formatted: Option<String>,

    /// Street address component.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub street_address: Option<String>,

    /// City or locality.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locality: Option<String>,

    /// State, province, prefecture, or region.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,

    /// Zip or postal code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub postal_code: Option<String>,

    /// Country name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
}

/// The claims carried in an OIDC ID token (Core 1.0 section 2).
///
/// Registered claims appear as named fields; the OIDC standard profile claims
/// are flattened in via [`StandardClaims`], and any other members land in
/// `additional`. The `aud` claim may be a single string or an array of strings
/// per the spec — see [`Audience`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct IdTokenClaims {
    /// REQUIRED. Issuer identifier.
    pub iss: String,

    /// REQUIRED. Subject identifier.
    pub sub: String,

    /// REQUIRED. Intended audience(s).
    pub aud: Audience,

    /// REQUIRED. Expiration time, seconds since the Unix epoch.
    pub exp: i64,

    /// REQUIRED. Issued-at time, seconds since the Unix epoch.
    pub iat: i64,

    /// Time of the end-user authentication, seconds since the Unix epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_time: Option<i64>,

    /// String value used to associate a client session with an ID token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,

    /// Authentication Context Class Reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acr: Option<String>,

    /// Authentication Methods References.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub amr: Vec<String>,

    /// Authorized party — the client the token was issued to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub azp: Option<String>,

    /// The OIDC standard claim set, flattened into the token body.
    #[serde(flatten)]
    pub standard: StandardClaims,

    /// Any additional claims not modeled above.
    #[serde(flatten)]
    pub additional: BTreeMap<String, Value>,
}

/// An ID token `aud` claim: either a single audience or a list.
///
/// OIDC permits both `"aud": "client-id"` and `"aud": ["a", "b"]`; this models
/// both shapes faithfully on the wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Audience {
    /// A single audience identifier.
    One(String),
    /// Multiple audience identifiers.
    Many(Vec<String>),
}

impl Default for Audience {
    fn default() -> Self {
        Audience::Many(Vec::new())
    }
}

impl Audience {
    /// Iterate over the audience values, regardless of wire shape.
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        match self {
            Audience::One(s) => std::slice::from_ref(s).iter().map(String::as_str),
            Audience::Many(v) => v.iter().map(String::as_str),
        }
    }

    /// Whether `candidate` is among the audiences.
    pub fn contains(&self, candidate: &str) -> bool {
        self.iter().any(|a| a == candidate)
    }
}

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
    pub fn with_nonce(mut self, nonce: impl Into<String>) -> Self {
        self.expected_nonce = Some(nonce.into());
        self
    }

    /// Set the allowed clock-skew leeway, in seconds.
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
        return Err(ProtocolError::AudienceMismatch {
            expected: opts.expected_audience.clone(),
        });
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

#[cfg(test)]
mod tests {
    use super::*;

    fn base_claims() -> IdTokenClaims {
        IdTokenClaims {
            iss: "https://issuer.example.com".into(),
            sub: "248289761001".into(),
            aud: Audience::One("s6BhdRkqt3".into()),
            exp: 2_000_000_000,
            iat: 1_000_000_000,
            ..Default::default()
        }
    }

    #[test]
    fn validate_id_token_happy_path() {
        let claims = base_claims();
        let opts = IdTokenValidation::new(
            "https://issuer.example.com",
            "s6BhdRkqt3",
            1_500_000_000,
        )
        .with_leeway(60);
        assert!(validate_id_token(&claims, &opts).is_ok());
    }

    #[test]
    fn validate_id_token_happy_path_with_nonce_and_array_aud() {
        let mut claims = base_claims();
        claims.aud = Audience::Many(vec!["other".into(), "s6BhdRkqt3".into()]);
        claims.nonce = Some("n-0S6_WzA2Mj".into());
        let opts = IdTokenValidation::new(
            "https://issuer.example.com",
            "s6BhdRkqt3",
            1_500_000_000,
        )
        .with_nonce("n-0S6_WzA2Mj");
        assert!(validate_id_token(&claims, &opts).is_ok());
    }

    #[test]
    fn validate_id_token_rejects_expired() {
        let claims = base_claims();
        // now is past exp.
        let opts =
            IdTokenValidation::new("https://issuer.example.com", "s6BhdRkqt3", 2_000_000_001);
        let err = validate_id_token(&claims, &opts).unwrap_err();
        assert!(matches!(err, ProtocolError::IdTokenExpired { .. }));
    }

    #[test]
    fn validate_id_token_expiry_respects_leeway() {
        let claims = base_claims();
        // now == exp would fail without leeway (exp must be strictly after).
        let opts =
            IdTokenValidation::new("https://issuer.example.com", "s6BhdRkqt3", 2_000_000_030)
                .with_leeway(60);
        assert!(validate_id_token(&claims, &opts).is_ok());
    }

    #[test]
    fn validate_id_token_rejects_wrong_issuer() {
        let claims = base_claims();
        let opts =
            IdTokenValidation::new("https://evil.example.com", "s6BhdRkqt3", 1_500_000_000);
        let err = validate_id_token(&claims, &opts).unwrap_err();
        match err {
            ProtocolError::IssuerMismatch { expected, actual } => {
                assert_eq!(expected, "https://evil.example.com");
                assert_eq!(actual, "https://issuer.example.com");
            }
            other => panic!("expected IssuerMismatch, got {other:?}"),
        }
    }

    #[test]
    fn validate_id_token_rejects_audience_not_containing_client() {
        let claims = base_claims();
        let opts = IdTokenValidation::new(
            "https://issuer.example.com",
            "different-client",
            1_500_000_000,
        );
        let err = validate_id_token(&claims, &opts).unwrap_err();
        assert!(matches!(err, ProtocolError::AudienceMismatch { .. }));
    }

    #[test]
    fn validate_id_token_rejects_nonce_mismatch() {
        let mut claims = base_claims();
        claims.nonce = Some("actual".into());
        let opts = IdTokenValidation::new(
            "https://issuer.example.com",
            "s6BhdRkqt3",
            1_500_000_000,
        )
        .with_nonce("expected");
        let err = validate_id_token(&claims, &opts).unwrap_err();
        assert!(matches!(err, ProtocolError::NonceMismatch));
    }

    #[test]
    fn validate_id_token_rejects_missing_nonce_when_required() {
        let claims = base_claims();
        let opts = IdTokenValidation::new(
            "https://issuer.example.com",
            "s6BhdRkqt3",
            1_500_000_000,
        )
        .with_nonce("expected");
        let err = validate_id_token(&claims, &opts).unwrap_err();
        assert!(matches!(err, ProtocolError::NonceMismatch));
    }

    #[test]
    fn validate_id_token_rejects_future_iat() {
        let mut claims = base_claims();
        claims.iat = 1_600_000_000;
        let opts =
            IdTokenValidation::new("https://issuer.example.com", "s6BhdRkqt3", 1_500_000_000);
        let err = validate_id_token(&claims, &opts).unwrap_err();
        assert!(matches!(err, ProtocolError::IdTokenNotYetValid { .. }));
    }

    #[test]
    fn provider_metadata_uses_exact_spec_keys() {
        let meta = ProviderMetadata {
            issuer: "https://issuer.example.com".into(),
            authorization_endpoint: Some("https://issuer.example.com/authorize".into()),
            token_endpoint: Some("https://issuer.example.com/token".into()),
            userinfo_endpoint: Some("https://issuer.example.com/userinfo".into()),
            jwks_uri: Some("https://issuer.example.com/jwks".into()),
            response_types_supported: vec![ResponseType::Code, ResponseType::CodeIdToken],
            grant_types_supported: vec![GrantType::AuthorizationCode, GrantType::RefreshToken],
            subject_types_supported: vec![SubjectType::Public],
            id_token_signing_alg_values_supported: vec!["RS256".into()],
            scopes_supported: vec![
                Scope::Known(KnownScope::OpenId),
                Scope::Known(KnownScope::Email),
                Scope::Other("custom".into()),
            ],
            claims_supported: vec!["sub".into(), "email".into()],
            ..Default::default()
        };

        let json = serde_json::to_value(&meta).unwrap();
        // Exact spec keys.
        assert!(json.get("issuer").is_some());
        assert!(json.get("response_types_supported").is_some());
        assert!(json.get("id_token_signing_alg_values_supported").is_some());
        assert!(json.get("subject_types_supported").is_some());
        // Enum string reps.
        assert_eq!(json["response_types_supported"][0], "code");
        assert_eq!(json["response_types_supported"][1], "code id_token");
        assert_eq!(json["grant_types_supported"][0], "authorization_code");
        assert_eq!(json["subject_types_supported"][0], "public");
        assert_eq!(json["scopes_supported"][0], "openid");
        assert_eq!(json["scopes_supported"][2], "custom");
        // Unset optional fields are omitted entirely.
        assert!(json.get("registration_endpoint").is_none());
        assert!(json.get("end_session_endpoint").is_none());
    }

    #[test]
    fn provider_metadata_round_trips() {
        let json = r#"{
            "issuer": "https://issuer.example.com",
            "authorization_endpoint": "https://issuer.example.com/authorize",
            "token_endpoint": "https://issuer.example.com/token",
            "jwks_uri": "https://issuer.example.com/jwks",
            "response_types_supported": ["code"],
            "subject_types_supported": ["public", "pairwise"],
            "id_token_signing_alg_values_supported": ["RS256", "ES256"],
            "scopes_supported": ["openid", "profile", "email"],
            "custom_extension": {"vendor": "klauthed"}
        }"#;
        let meta: ProviderMetadata = serde_json::from_str(json).unwrap();
        assert_eq!(meta.issuer, "https://issuer.example.com");
        assert_eq!(meta.subject_types_supported.len(), 2);
        assert_eq!(meta.subject_types_supported[1], SubjectType::Pairwise);
        // Unmodeled members are preserved.
        assert!(meta.additional.contains_key("custom_extension"));

        let reser = serde_json::to_value(&meta).unwrap();
        assert_eq!(reser["custom_extension"]["vendor"], "klauthed");
    }

    #[test]
    fn standard_claims_field_names() {
        let claims = StandardClaims {
            sub: Some("248289761001".into()),
            name: Some("Jane Doe".into()),
            given_name: Some("Jane".into()),
            family_name: Some("Doe".into()),
            preferred_username: Some("j.doe".into()),
            email: Some("janedoe@example.com".into()),
            email_verified: Some(true),
            locale: Some("en-US".into()),
            updated_at: Some(1_700_000_000),
            ..Default::default()
        };
        let json = serde_json::to_value(&claims).unwrap();
        assert_eq!(json["given_name"], "Jane");
        assert_eq!(json["family_name"], "Doe");
        assert_eq!(json["preferred_username"], "j.doe");
        assert_eq!(json["email_verified"], true);
        assert_eq!(json["updated_at"], 1_700_000_000);
        // gender unset -> omitted.
        assert!(json.get("gender").is_none());
    }

    #[test]
    fn id_token_flattens_standard_claims_and_extras() {
        let json = r#"{
            "iss": "https://issuer.example.com",
            "sub": "248289761001",
            "aud": "s6BhdRkqt3",
            "exp": 1311281970,
            "iat": 1311280970,
            "auth_time": 1311280969,
            "nonce": "n-0S6_WzA2Mj",
            "acr": "urn:mace:incommon:iap:silver",
            "amr": ["pwd", "otp"],
            "azp": "s6BhdRkqt3",
            "email": "janedoe@example.com",
            "email_verified": true,
            "name": "Jane Doe",
            "groups": ["admins"]
        }"#;
        let claims: IdTokenClaims = serde_json::from_str(json).unwrap();
        assert_eq!(claims.iss, "https://issuer.example.com");
        assert_eq!(claims.aud, Audience::One("s6BhdRkqt3".into()));
        assert!(claims.aud.contains("s6BhdRkqt3"));
        assert_eq!(claims.amr, vec!["pwd", "otp"]);
        // Flattened standard claims.
        assert_eq!(claims.standard.email.as_deref(), Some("janedoe@example.com"));
        assert_eq!(claims.standard.email_verified, Some(true));
        assert_eq!(claims.standard.name.as_deref(), Some("Jane Doe"));
        // Flattened extras.
        assert_eq!(claims.additional["groups"][0], "admins");

        // Round-trip preserves the flat shape (no nested "standard" object).
        let reser = serde_json::to_value(&claims).unwrap();
        assert_eq!(reser["email"], "janedoe@example.com");
        assert_eq!(reser["name"], "Jane Doe");
        assert_eq!(reser["groups"][0], "admins");
        assert!(reser.get("standard").is_none());
        assert!(reser.get("additional").is_none());
    }

    #[test]
    fn audience_supports_array_form() {
        let claims: IdTokenClaims = serde_json::from_str(
            r#"{"iss":"i","sub":"s","aud":["a","b"],"exp":1,"iat":0}"#,
        )
        .unwrap();
        assert_eq!(claims.aud, Audience::Many(vec!["a".into(), "b".into()]));
        let reser = serde_json::to_value(&claims).unwrap();
        assert_eq!(reser["aud"], serde_json::json!(["a", "b"]));
    }
}
