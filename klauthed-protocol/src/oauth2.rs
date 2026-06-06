//! OAuth 2.0 message types (RFC 6749) and OIDC authorization/token messages.
//!
//! Spec-accurate serde models for the authorization request, the token-endpoint
//! request and response, and the token-endpoint error response. These are pure
//! data types: no flows are executed, no tokens are signed or verified, and no
//! HTTP is performed. ID-token *claim* validation (no crypto) lives alongside
//! the claim types in [`crate::oidc`]; signature verification lives in
//! `klauthed-security`.
//!
//! Field names match the wire format exactly.
//!
//! # Encoding
//!
//! Per RFC 6749, the [`AuthorizationRequest`] travels as query parameters and
//! the [`TokenRequest`] travels as an `application/x-www-form-urlencoded`
//! request body, while [`TokenResponse`] / [`TokenErrorResponse`] are JSON.
//! These types model the *fields* with their exact spec names; they derive
//! serde so they work with JSON directly and with a form-encoder such as
//! `serde_urlencoded` for the request side. Scopes are a single
//! space-delimited string on the wire (see [`scope_to_string`] /
//! [`scope_from_str`]).
//!
//! References:
//! * RFC 6749 — The OAuth 2.0 Authorization Framework
//! * RFC 7636 — Proof Key for Code Exchange (PKCE)
//! * OpenID Connect Core 1.0 (sections 3.1.2, 3.1.3)
//!
//! ```
//! use klauthed_protocol::oauth2::{AuthorizationRequest, CodeChallengeMethod};
//! use klauthed_protocol::oidc::ResponseType;
//!
//! let req = AuthorizationRequest {
//!     response_type: ResponseType::Code,
//!     client_id: "s6BhdRkqt3".into(),
//!     redirect_uri: Some("https://rp.example.com/cb".into()),
//!     scope: Some("openid email".into()),
//!     state: Some("xyz".into()),
//!     code_challenge: Some("E9Me...".into()),
//!     code_challenge_method: Some(CodeChallengeMethod::S256),
//!     ..Default::default()
//! };
//! let json = serde_json::to_value(&req).unwrap();
//! assert_eq!(json["response_type"], "code");
//! assert_eq!(json["code_challenge_method"], "S256");
//! ```

use serde::{Deserialize, Serialize};

use crate::oidc::{GrantType, ResponseType};

/// The PKCE code challenge method (RFC 7636 section 4.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum CodeChallengeMethod {
    /// `plain` — the challenge equals the verifier.
    #[serde(rename = "plain")]
    Plain,
    /// `S256` — the challenge is `BASE64URL(SHA256(verifier))`.
    #[serde(rename = "S256")]
    S256,
}

/// The OAuth 2.0 access token type (RFC 6749 section 7.1).
///
/// `Bearer` is by far the most common; `Other` preserves any other registered
/// type. The wire form is matched case-insensitively on the well-known values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TokenType {
    /// A well-known token type.
    Known(KnownTokenType),
    /// Any other token type token.
    Other(String),
}

/// Well-known OAuth 2.0 token types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum KnownTokenType {
    /// `Bearer` (RFC 6750). Serializes to the exact spec casing `"Bearer"`.
    #[serde(rename = "Bearer", alias = "bearer")]
    Bearer,
    /// `DPoP` (RFC 9449).
    #[serde(rename = "DPoP", alias = "dpop")]
    DPoP,
}

impl Default for TokenType {
    fn default() -> Self {
        TokenType::Known(KnownTokenType::Bearer)
    }
}

/// The OIDC `prompt` authorization-request parameter (Core 1.0 section 3.1.2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Prompt {
    /// `none` — do not display any UI; error if interaction is required.
    #[serde(rename = "none")]
    None,
    /// `login` — reauthenticate the end-user.
    #[serde(rename = "login")]
    Login,
    /// `consent` — prompt for consent before returning information.
    #[serde(rename = "consent")]
    Consent,
    /// `select_account` — prompt to select a user account.
    #[serde(rename = "select_account")]
    SelectAccount,
}

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
        Self {
            error,
            error_description: None,
            error_uri: None,
        }
    }

    /// Construct an error response with a human-readable description.
    pub fn with_description(error: OAuth2ErrorCode, description: impl Into<String>) -> Self {
        Self {
            error,
            error_description: Some(description.into()),
            error_uri: None,
        }
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

/// Join a list of scope tokens into the space-delimited wire form
/// (RFC 6749 section 3.3).
pub fn scope_to_string<I, S>(scopes: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut out = String::new();
    for s in scopes {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(s.as_ref());
    }
    out
}

/// Split the space-delimited `scope` wire form into individual tokens,
/// dropping empty segments (RFC 6749 section 3.3).
pub fn scope_from_str(scope: &str) -> Vec<String> {
    scope
        .split(' ')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorization_request_uses_exact_spec_keys() {
        let req = AuthorizationRequest {
            response_type: ResponseType::Code,
            client_id: "s6BhdRkqt3".into(),
            redirect_uri: Some("https://rp.example.com/cb".into()),
            scope: Some("openid email".into()),
            state: Some("xyz".into()),
            nonce: Some("n-0S6_WzA2Mj".into()),
            code_challenge: Some("E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM".into()),
            code_challenge_method: Some(CodeChallengeMethod::S256),
            ..Default::default()
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["response_type"], "code");
        assert_eq!(json["client_id"], "s6BhdRkqt3");
        assert_eq!(json["redirect_uri"], "https://rp.example.com/cb");
        assert_eq!(json["scope"], "openid email");
        assert_eq!(json["state"], "xyz");
        assert_eq!(json["code_challenge_method"], "S256");
        // Unset optionals omitted.
        assert!(json.get("max_age").is_none());
        assert!(json.get("login_hint").is_none());
    }

    #[test]
    fn code_challenge_method_plain() {
        let json = serde_json::to_value(CodeChallengeMethod::Plain).unwrap();
        assert_eq!(json, "plain");
        let back: CodeChallengeMethod = serde_json::from_value(json).unwrap();
        assert_eq!(back, CodeChallengeMethod::Plain);
    }

    #[test]
    fn token_request_uses_exact_spec_keys() {
        let req = TokenRequest {
            grant_type: GrantType::AuthorizationCode,
            code: Some("SplxlOBeZQQYbYS6WxSbIA".into()),
            redirect_uri: Some("https://rp.example.com/cb".into()),
            client_id: Some("s6BhdRkqt3".into()),
            code_verifier: Some("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk".into()),
            ..Default::default()
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["grant_type"], "authorization_code");
        assert_eq!(json["code"], "SplxlOBeZQQYbYS6WxSbIA");
        assert_eq!(json["redirect_uri"], "https://rp.example.com/cb");
        assert_eq!(json["client_id"], "s6BhdRkqt3");
        assert_eq!(json["code_verifier"], "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk");
        // Unused fields omitted.
        assert!(json.get("refresh_token").is_none());
        assert!(json.get("client_secret").is_none());
    }

    #[test]
    fn token_request_refresh_round_trips() {
        let json = r#"{"grant_type":"refresh_token","refresh_token":"tGzv3JOkF0XG5Qx2TlKWIA","scope":"openid"}"#;
        let req: TokenRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.grant_type, GrantType::RefreshToken);
        assert_eq!(req.refresh_token.as_deref(), Some("tGzv3JOkF0XG5Qx2TlKWIA"));
        assert_eq!(req.scope.as_deref(), Some("openid"));
        let reser = serde_json::to_value(&req).unwrap();
        assert_eq!(reser["grant_type"], "refresh_token");
        assert!(reser.get("code").is_none());
    }

    #[test]
    fn token_response_uses_exact_spec_keys() {
        let resp = TokenResponse {
            access_token: "2YotnFZFEjr1zCsicMWpAA".into(),
            token_type: TokenType::default(),
            expires_in: Some(3600),
            refresh_token: Some("tGzv3JOkF0XG5Qx2TlKWIA".into()),
            id_token: Some("eyJ...".into()),
            scope: Some("openid email".into()),
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["access_token"], "2YotnFZFEjr1zCsicMWpAA");
        assert_eq!(json["token_type"], "Bearer");
        assert_eq!(json["expires_in"], 3600);
        assert_eq!(json["refresh_token"], "tGzv3JOkF0XG5Qx2TlKWIA");
        assert_eq!(json["id_token"], "eyJ...");
        assert_eq!(json["scope"], "openid email");
    }

    #[test]
    fn token_response_round_trips_and_omits_absent() {
        let json = r#"{"access_token":"abc","token_type":"Bearer"}"#;
        let resp: TokenResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.access_token, "abc");
        assert_eq!(resp.token_type, TokenType::Known(KnownTokenType::Bearer));
        assert!(resp.id_token.is_none());
        let reser = serde_json::to_value(&resp).unwrap();
        assert!(reser.get("expires_in").is_none());
        assert!(reser.get("id_token").is_none());
    }

    #[test]
    fn token_type_accepts_lowercase_and_custom() {
        let lower: TokenType = serde_json::from_str("\"bearer\"").unwrap();
        assert_eq!(lower, TokenType::Known(KnownTokenType::Bearer));
        let custom: TokenType = serde_json::from_str("\"mac\"").unwrap();
        assert_eq!(custom, TokenType::Other("mac".into()));
    }

    #[test]
    fn error_codes_serialize_to_spec_strings() {
        assert_eq!(
            serde_json::to_value(OAuth2ErrorCode::InvalidRequest).unwrap(),
            "invalid_request"
        );
        assert_eq!(
            serde_json::to_value(OAuth2ErrorCode::InvalidClient).unwrap(),
            "invalid_client"
        );
        assert_eq!(
            serde_json::to_value(OAuth2ErrorCode::InvalidGrant).unwrap(),
            "invalid_grant"
        );
        assert_eq!(
            serde_json::to_value(OAuth2ErrorCode::UnauthorizedClient).unwrap(),
            "unauthorized_client"
        );
        assert_eq!(
            serde_json::to_value(OAuth2ErrorCode::UnsupportedGrantType).unwrap(),
            "unsupported_grant_type"
        );
        assert_eq!(
            serde_json::to_value(OAuth2ErrorCode::InvalidScope).unwrap(),
            "invalid_scope"
        );
    }

    #[test]
    fn token_error_response_shape() {
        let err = TokenErrorResponse::with_description(
            OAuth2ErrorCode::InvalidGrant,
            "authorization code expired",
        );
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["error"], "invalid_grant");
        assert_eq!(json["error_description"], "authorization code expired");
        assert!(json.get("error_uri").is_none());

        let parsed: TokenErrorResponse = serde_json::from_value(json).unwrap();
        assert_eq!(parsed.error, OAuth2ErrorCode::InvalidGrant);

        let bare = TokenErrorResponse::new(OAuth2ErrorCode::InvalidClient);
        let bare_json = serde_json::to_value(&bare).unwrap();
        assert_eq!(bare_json["error"], "invalid_client");
        assert!(bare_json.get("error_description").is_none());
    }

    #[test]
    fn scope_helpers_round_trip() {
        assert_eq!(scope_to_string(["openid", "email", "profile"]), "openid email profile");
        assert_eq!(scope_to_string(Vec::<String>::new()), "");
        assert_eq!(
            scope_from_str("openid  email "),
            vec!["openid".to_string(), "email".to_string()]
        );
        assert_eq!(scope_from_str(""), Vec::<String>::new());
    }
}
