//! JSON Web Tokens (signing + verification).
//!
//! Wraps the vetted [`jsonwebtoken`] crate with a klauthed-flavoured API:
//!
//! * [`Claims`] — the standard registered claims plus a bag of custom claims.
//! * [`JwtSigner`] — encodes [`Claims`] into a compact JWT.
//! * [`JwtVerifier`] — decodes + validates a JWT back into [`Claims`].
//!
//! HS256 (shared secret), RS256 (RSA), ES256 (ECDSA P-256), and EdDSA (Ed25519)
//! are supported; asymmetric keys load from PEM or DER. Expiry is computed from a
//! [`Clock`](klauthed_core::time::Clock) so it stays testable.
//!
//! ```
//! use klauthed_security::jwt::{Claims, JwtSigner, JwtVerifier};
//! use klauthed_core::time::SystemClock;
//! use klauthed_core::time::Duration;
//!
//! let signer = JwtSigner::hs256(b"super-secret-signing-key");
//! let verifier = JwtVerifier::hs256(b"super-secret-signing-key");
//!
//! let claims = Claims::builder("user-123", &SystemClock, Duration::minutes(15))
//!     .issuer("klauthed")
//!     .audience("klauthed-api")
//!     .build();
//!
//! let token = signer.encode(&claims).unwrap();
//! let decoded = verifier
//!     .expecting_issuer("klauthed")
//!     .expecting_audience("klauthed-api")
//!     .decode(&token)
//!     .unwrap();
//! assert_eq!(decoded.sub.as_deref(), Some("user-123"));
//! ```

pub mod claims;
pub mod signer;
pub mod verifier;

#[cfg(test)]
mod proptests;

pub use claims::{Claims, ClaimsBuilder};
pub use signer::JwtSigner;
pub use verifier::JwtVerifier;
