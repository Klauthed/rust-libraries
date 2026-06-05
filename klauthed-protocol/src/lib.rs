#![deny(unsafe_code)]

//! Spec-accurate serde data types for the identity protocols klauthed speaks.
//!
//! This crate is *typed data modeling*: it defines the wire shapes so the rest
//! of the system can serialize and deserialize them with field names matching
//! the relevant specs exactly. It implements **no** network I/O, OAuth flows,
//! token validation crypto, or HTTP clients — those belong to other crates
//! (JWT signing/verification lives in `klauthed-security`).
//!
//! Protocol families live behind independent features (all on by default):
//!
//! * [`oidc`] — OpenID Connect discovery metadata and claim types, plus
//!   claim-level ID-token validation (feature `oidc`).
//! * [`jwks`] — JSON Web Key / Key Set types (RFC 7517) and key lookup
//!   (feature `oidc`). Signature verification against a selected key lives in
//!   `klauthed-security`.
//! * [`oauth2`] — OAuth 2.0 (RFC 6749) authorization/token message types
//!   (feature `oauth2`, which implies `oidc`).
//! * [`scim`] — SCIM 2.0 (RFC 7643/7644) core resource types
//!   (feature `scim`).
//!
//! Parse/validation failures surface as [`ProtocolError`], which implements
//! `klauthed_error::DomainError`.
//!
//! # Out of scope
//!
//! ID-token *signature* verification, JWKS fetching, JWT decoding, OAuth flow
//! execution, SCIM PATCH application semantics, and HTTP transport are
//! intentionally out of scope. ID-token validation here is **claim-level only**
//! (`iss`/`aud`/`exp`/`iat`/`nonce`); signature verification lives in
//! `klauthed-security`. These types build into flows elsewhere.

mod error;

pub use error::ProtocolError;

#[cfg(feature = "oidc")]
pub mod jwks;

#[cfg(feature = "oidc")]
pub mod oidc;

#[cfg(feature = "oidc")]
pub mod oauth2;

#[cfg(feature = "scim")]
pub mod scim;
