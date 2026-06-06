//! OAuth 2.0 request/response message types: the authorization and token
//! endpoints (RFC 6749) plus revocation (RFC 7009) and introspection
//! (RFC 7662).

use serde::{Deserialize, Serialize};

use crate::oidc::{GrantType, ResponseType};

use super::{CodeChallengeMethod, TokenType};

/// An OAuth 2.0 / OIDC authorization request (RFC 6749 section 4.1.1,
/// OIDC Core section 3.1.2.1).
///
/// On the wire these are query parameters on the authorization endpoint;
/// `scope` is a single space-delimited string. Optional parameters are skipped
/// when `None`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorizationRequest {
    /// REQUIRED. The response type (`code`, `id_token`, …).
    pub response_type: ResponseType,

    /// REQUIRED. The client identifier.
    pub client_id: String,

    /// The redirection URI to return the response to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redirect_uri: Option<String>,

    /// The requested scope as a single space-delimited string (e.g.
    /// `"openid email"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,

    /// Opaque value used to maintain state and mitigate CSRF.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,

    /// String value used to associate a client session with an ID token and
    /// mitigate replay (OIDC).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,

    /// PKCE code challenge (RFC 7636).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_challenge: Option<String>,

    /// PKCE code challenge method (RFC 7636).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_challenge_method: Option<CodeChallengeMethod>,

    /// How the result should be returned (`query`, `fragment`, `form_post`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_mode: Option<String>,

    /// Space-delimited list of prompt behaviors (OIDC). A single `Prompt`
    /// value, modeled here as a string to allow space-delimited combinations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,

    /// Maximum authentication age, in seconds (OIDC).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_age: Option<i64>,

    /// Requested Authentication Context Class Reference values, space-delimited
    /// (OIDC).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acr_values: Option<String>,

    /// Hint about the login identifier the end-user might use (OIDC).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login_hint: Option<String>,

    /// End-user's preferred languages for the UI, space-delimited BCP47 tags
    /// (OIDC).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui_locales: Option<String>,
}

/// An OAuth 2.0 token-endpoint request (RFC 6749 sections 4.1.3, 4.3, 6;
/// OIDC Core section 3.1.3.1).
///
/// On the wire this is an `application/x-www-form-urlencoded` request body.
/// Which fields are present depends on `grant_type`: `authorization_code`
/// carries `code` (+ `redirect_uri`, `code_verifier`), `refresh_token` carries
/// `refresh_token`, and so on. Optional fields are skipped when `None`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenRequest {
    /// REQUIRED. The grant type being exercised.
    pub grant_type: GrantType,

    /// The authorization code (for `grant_type=authorization_code`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,

    /// The redirection URI, if one was included in the authorization request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redirect_uri: Option<String>,

    /// The client identifier (when not sent via HTTP auth).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,

    /// The client secret (for confidential clients using form-body auth).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,

    /// The refresh token (for `grant_type=refresh_token`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,

    /// The PKCE code verifier (RFC 7636).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_verifier: Option<String>,

    /// The requested scope as a single space-delimited string (e.g. when
    /// narrowing scope on `grant_type=refresh_token`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,

    /// The resource owner username (for `grant_type=password`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,

    /// The resource owner password (for `grant_type=password`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}

/// A successful OAuth 2.0 token-endpoint response (RFC 6749 section 5.1).
///
/// Serialized as JSON. `id_token` is the OIDC addition (Core 3.1.3.3). Optional
/// fields are skipped when absent.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenResponse {
    /// REQUIRED. The issued access token.
    pub access_token: String,

    /// REQUIRED. The token type (typically `Bearer`).
    pub token_type: TokenType,

    /// The access token lifetime, in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_in: Option<i64>,

    /// A refresh token usable to obtain new access tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,

    /// The OIDC ID token (a signed JWT, opaque to this crate).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id_token: Option<String>,

    /// The granted scope, space-delimited, if it differs from what was
    /// requested.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

/// The standard OAuth 2.0 token-endpoint error codes (RFC 6749 section 5.2).
///
/// Serializes to the exact snake_case spec strings (`"invalid_grant"`, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum OAuth2ErrorCode {
    /// The request is missing a parameter, malformed, or otherwise invalid.
    InvalidRequest,
    /// Client authentication failed.
    InvalidClient,
    /// The provided authorization grant or refresh token is invalid.
    InvalidGrant,
    /// The authenticated client is not authorized to use this grant type.
    UnauthorizedClient,
    /// The grant type is not supported by the authorization server.
    UnsupportedGrantType,
    /// The requested scope is invalid, unknown, or malformed.
    InvalidScope,
    /// The resource owner or authorization server denied the request
    /// (authorization endpoint; RFC 6749 section 4.1.2.1).
    AccessDenied,
    /// The authorization server does not support this response type
    /// (authorization endpoint).
    UnsupportedResponseType,
    /// The authorization server encountered an unexpected condition
    /// (authorization endpoint).
    ServerError,
    /// The authorization server is temporarily unable to handle the request
    /// (authorization endpoint).
    TemporarilyUnavailable,
}

/// An OAuth 2.0 error response (RFC 6749 sections 5.2 and 4.1.2.1).
///
/// Serialized as JSON at the token endpoint, or as redirect query/fragment
/// parameters at the authorization endpoint. Optional fields are skipped when
/// absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenErrorResponse {
    /// REQUIRED. A single error code.
    pub error: OAuth2ErrorCode,

    /// Human-readable text providing additional information.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_description: Option<String>,

    /// A URI identifying a human-readable web page with error information.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_uri: Option<String>,
}

impl TokenErrorResponse {
    /// Construct an error response carrying just an error code.
    pub fn new(error: OAuth2ErrorCode) -> Self {
        Self { error, error_description: None, error_uri: None }
    }

    /// Construct an error response with a human-readable description.
    pub fn with_description(error: OAuth2ErrorCode, description: impl Into<String>) -> Self {
        Self { error, error_description: Some(description.into()), error_uri: None }
    }
}

/// A hint about which kind of token is being revoked or introspected
/// (RFC 7009 section 2.1, RFC 7662 section 2.1).
///
/// Serializes to the snake_case wire strings `"access_token"` /
/// `"refresh_token"`. The server treats it as advisory: it MAY ignore the hint
/// and check both token types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenTypeHint {
    /// The token is an access token.
    AccessToken,
    /// The token is a refresh token.
    RefreshToken,
}

/// A token revocation request (RFC 7009 section 2.1).
///
/// Sent `application/x-www-form-urlencoded` to the revocation endpoint. The
/// client authenticates as it would at the token endpoint (HTTP Basic, or the
/// `client_id` / `client_secret` form fields modeled here).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevocationRequest {
    /// REQUIRED. The token the client wants revoked.
    pub token: String,

    /// OPTIONAL. A hint about the type of `token`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_type_hint: Option<TokenTypeHint>,

    /// The client identifier (when not sent via HTTP auth).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,

    /// The client secret (for confidential clients using form-body auth).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
}

/// A token introspection request (RFC 7662 section 2.1).
///
/// Same wire shape as a [`RevocationRequest`]; modeled separately for clarity.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntrospectionRequest {
    /// REQUIRED. The token to introspect.
    pub token: String,

    /// OPTIONAL. A hint about the type of `token`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_type_hint: Option<TokenTypeHint>,

    /// The client identifier (when not sent via HTTP auth).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,

    /// The client secret (for confidential clients using form-body auth).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
}

/// A token introspection response (RFC 7662 section 2.2).
///
/// `active` is the only REQUIRED member. For an inactive (or unknown) token the
/// server returns `{"active": false}` and nothing else, to avoid leaking
/// information. The remaining members are populated only for active tokens.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntrospectionResponse {
    /// REQUIRED. Whether the token is currently active.
    pub active: bool,

    /// Space-delimited list of scopes associated with the token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,

    /// The client the token was issued to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,

    /// Subject — the user the token was issued for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub: Option<String>,

    /// Token type (typically `Bearer`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_type: Option<TokenType>,

    /// Expiration time (seconds since the Unix epoch).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exp: Option<i64>,

    /// Issued-at time (seconds since the Unix epoch).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iat: Option<i64>,

    /// Issuer of the token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iss: Option<String>,

    /// The token's unique identifier (`jti`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jti: Option<String>,
}

impl IntrospectionResponse {
    /// The canonical inactive response: `{"active": false}`.
    pub fn inactive() -> Self {
        Self::default()
    }
}
