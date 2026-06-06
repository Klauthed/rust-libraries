//! OIDC discovery vocabulary (response/grant/subject types, scopes) and the
//! [`ProviderMetadata`] document.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

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
