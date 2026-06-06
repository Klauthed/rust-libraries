//! OAuth 2.0 value enums: [`CodeChallengeMethod`], [`TokenType`], and [`Prompt`].

use serde::{Deserialize, Serialize};

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
