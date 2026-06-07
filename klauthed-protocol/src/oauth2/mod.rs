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

pub mod messages;
pub mod params;
pub mod scope;

pub use messages::{
    AuthorizationRequest, IntrospectionRequest, IntrospectionResponse, OAuth2ErrorCode,
    RevocationRequest, TokenErrorResponse, TokenRequest, TokenResponse, TokenTypeHint,
};
pub use params::{CodeChallengeMethod, KnownTokenType, Prompt, TokenType};
pub use scope::{scope_from_str, scope_to_string};
