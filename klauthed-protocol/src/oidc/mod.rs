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

pub mod claims;
pub mod metadata;
pub mod validation;

pub use claims::{AddressClaim, Audience, IdTokenClaims, StandardClaims};
pub use metadata::{GrantType, KnownScope, ProviderMetadata, ResponseType, Scope, SubjectType};
pub use validation::{IdTokenValidation, validate_id_token};

#[cfg(test)]
mod tests;
