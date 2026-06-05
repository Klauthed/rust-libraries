#![deny(unsafe_code)]
#![deny(missing_docs)]

//! Security primitives for klauthed.
//!
//! A small, focused toolkit built entirely on vetted cryptographic crates — no
//! hand-rolled primitives. It covers the four building blocks most services
//! need:
//!
//! * **Password hashing** ([`password`]) — Argon2id PHC strings:
//!   [`hash_password`] /
//!   [`verify_password`].
//! * **JWTs** ([`jwt`]) — [`Claims`] with a [`JwtSigner`]
//!   and [`JwtVerifier`] supporting HS256 and RS256, with
//!   `exp`/`iss`/`aud`/`nbf` validation and `exp` derived from a
//!   [`Clock`](klauthed_core::time::Clock).
//! * **Secure random tokens** ([`token`]) —
//!   [`random_token`] /
//!   [`random_bytes`] from the OS CSPRNG.
//! * **Constant-time comparison** ([`compare`]) —
//!   [`constant_time_eq`] for secret/MAC equality.
//!
//! * **Sessions** ([`session`]) — opaque server-side
//!   [`Session`]s behind an async [`SessionStore`] trait, with an
//!   [`InMemorySessionStore`] whose expiry is driven by an injected
//!   [`Clock`](klauthed_core::time::Clock).
//! * **Authorization / RBAC** ([`authz`]) — [`Permission`]s (with
//!   `users:*` / `*` wildcards), [`Role`]s, a [`RoleRegistry`], and an
//!   [`Authorizer`] policy checker.
//! * **MFA / TOTP** ([`mfa`]) — [RFC 6238] one-time passwords:
//!   generate a [`TotpSecret`], build the `otpauth://` URI, and verify codes.
//! * **AEAD encryption** ([`aead`]) — authenticated symmetric encryption with
//!   AES-256-GCM: an [`EncryptionKey`] plus [`encrypt`] / [`decrypt`] with a
//!   per-message random nonce prepended to the ciphertext.
//! * **Key derivation** ([`kdf`]) — HKDF-SHA256
//!   ([`derive_key`] / [`derive_key_32`]) for deriving purpose-specific
//!   subkeys from a root secret.
//! * **API keys** ([`apikey`]) — [`generate_api_key`] /
//!   [`verify_api_key`] for high-entropy bearer credentials (SHA-256 verifier,
//!   constant-time compare).
//!
//! [RFC 6238]: https://datatracker.ietf.org/doc/html/rfc6238
//!
//! All fallible operations return [`SecurityError`], which implements
//! [`klauthed_error::DomainError`] so it slots into the shared error handling
//! (HTTP status, retryability, stable `security.*` codes).
//!
//! # Not (yet) included
//!
//! WebAuthn/passkeys, OAuth/OIDC, ABAC / a general policy engine, persistent
//! (DB/Redis-backed) session stores, the Vault client, envelope encryption /
//! KMS, and asymmetric encryption are intentionally out of scope for this pass
//! and may land later or in dedicated crates.

pub mod aead;
pub mod apikey;
pub mod authz;
pub mod compare;
pub mod error;
pub mod jwt;
pub mod kdf;
pub mod mfa;
pub mod password;
pub mod session;
pub mod token;

pub use aead::{
    decrypt, decrypt_from_base64, encrypt, encrypt_to_base64, EncryptionKey, KEY_LEN,
};
pub use apikey::{generate_api_key, verify_api_key};
pub use authz::{Authorizer, Permission, Role, RoleRegistry};
pub use compare::constant_time_eq;
pub use error::SecurityError;
pub use jwt::{Claims, ClaimsBuilder, JwtSigner, JwtVerifier};
pub use kdf::{derive_key, derive_key_32};
pub use mfa::{Totp, TotpSecret};
pub use password::{hash_password, verify_password};
pub use session::{InMemorySessionStore, Session, SessionId, SessionStore};
pub use token::{random_bytes, random_token};
