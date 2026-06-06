//! JSON Web Key (JWK) and JSON Web Key Set (JWKS) data types (RFC 7517).
//!
//! Spec-accurate serde models for the documents an OIDC/OAuth 2.0 authorization
//! server publishes at its `jwks_uri`. These are pure *data* types plus
//! in-memory lookup: one [`JsonWebKey`] struct round-trips any single JWK
//! (RSA, EC, `oct`, OKP) because the key-type-specific material is modeled as
//! optional fields, and a [`JsonWebKeySet`] is a list of them with helpers for
//! the "pick the key for this token" case.
//!
//! Field names match the JSON wire format exactly. The one collision with a
//! Rust keyword is the `use` parameter, exposed here as
//! [`JsonWebKey::key_use`] (`#[serde(rename = "use")]`).
//!
//! # Out of scope
//!
//! This crate does **no** cryptography. Converting a [`JsonWebKey`] into a
//! concrete public/secret verification key and checking a JWT signature against
//! it is the job of `klauthed-security`, which consumes a key selected here
//! (e.g. via [`JsonWebKeySet::select`]). Nothing in this module fetches JWKS
//! over HTTP, decodes JWTs, or validates signatures.
//!
//! References:
//! * RFC 7517 (JSON Web Key)
//! * RFC 7518 (JSON Web Algorithms — `kty`, `crv`, parameter names)
//!
//! ```
//! use klauthed_protocol::jwks::{JsonWebKeySet, KeyType};
//!
//! let raw = r#"{"keys":[
//!   {"kty":"RSA","use":"sig","kid":"abc","alg":"RS256","n":"0vx7...","e":"AQAB"}
//! ]}"#;
//! let set: JsonWebKeySet = serde_json::from_str(raw).unwrap();
//! let key = set.find("abc").unwrap();
//! assert_eq!(key.kind(), Some(KeyType::Rsa));
//! assert_eq!(key.e.as_deref(), Some("AQAB"));
//! ```

pub mod key;
pub mod set;

pub use key::{JsonWebKey, KeyType, PublicKeyUse};
pub use set::JsonWebKeySet;

#[cfg(test)]
mod tests;
