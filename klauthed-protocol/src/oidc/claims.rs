//! OIDC claim sets: [`StandardClaims`], [`AddressClaim`], [`IdTokenClaims`],
//! and the [`Audience`] helper.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

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
