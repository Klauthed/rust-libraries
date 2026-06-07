//! API key generation and verification.
//!
//! An API key is a long-lived bearer credential a client sends on each request.
//! Unlike a user password, it is **high-entropy** (here, 32 bytes / 256 bits
//! from the OS CSPRNG), so it does not need — and should not get — an expensive
//! password hash like Argon2: there is no low-entropy secret to brute-force, and
//! a slow hash would only tax your own request path. Instead the stored
//! verifier is a single SHA-256 digest, which is fast and lets verification
//! run on every request cheaply.
//!
//! Keys are formatted as:
//!
//! ```text
//! {prefix}_{urlsafe-base64 of 32 random bytes}
//! ```
//!
//! The `prefix` (e.g. `"sk"`, `"pk_live"`) is a non-secret human/route hint and
//! is included in the hashed material. [`generate_api_key`] returns the
//! plaintext key (show it to the user **once**) and the hex SHA-256 hash to
//! persist. [`verify_api_key`] re-hashes a presented key and compares it to the
//! stored hash in constant time via [`crate::compare::constant_time_eq`].
//!
//! ```
//! use klauthed_security::apikey::{generate_api_key, verify_api_key};
//!
//! let (key, stored_hash) = generate_api_key("sk").unwrap();
//! assert!(key.starts_with("sk_"));
//!
//! // Persist `stored_hash`; show `key` to the user once.
//! assert!(verify_api_key(&key, &stored_hash));
//! assert!(!verify_api_key("sk_wrong", &stored_hash));
//! ```
//!
//! # Note
//!
//! SHA-256 is appropriate *because* the key is high-entropy. Never store a
//! user-chosen password this way — use [`crate::password`] (Argon2id) instead.

use ring::digest::{SHA256, digest};

use crate::compare::constant_time_eq;
use crate::error::SecurityError;
use crate::token::random_token;

/// Number of random bytes of entropy in a generated API key (256 bits).
const KEY_ENTROPY_BYTES: usize = 32;

/// Hex-encode the SHA-256 digest of `key`.
fn sha256_hex(key: &str) -> String {
    hex::encode(digest(&SHA256, key.as_bytes()).as_ref())
}

/// Generate a new API key with the given `prefix`.
///
/// Returns `(key, stored_hash)`:
///
/// * `key` — the full plaintext credential `"{prefix}_{base64url(32 bytes)}"`;
///   show this to the user **once**, it is not recoverable from the hash.
/// * `stored_hash` — the lowercase hex SHA-256 of `key`; persist this and use it
///   with [`verify_api_key`].
///
/// # Errors
///
/// Returns [`SecurityError::Rng`] if the OS CSPRNG fails.
pub fn generate_api_key(prefix: &str) -> Result<(String, String), SecurityError> {
    let secret = random_token(KEY_ENTROPY_BYTES)?;
    let key = format!("{prefix}_{secret}");
    let stored_hash = sha256_hex(&key);
    Ok((key, stored_hash))
}

/// Verify a `presented` API key against a previously stored hash.
///
/// Re-hashes `presented` with SHA-256 and compares against `stored_hash` in
/// constant time. Returns `false` for a wrong key or a malformed/tampered
/// stored hash; it never errors.
#[must_use]
pub fn verify_api_key(presented: &str, stored_hash: &str) -> bool {
    let computed = sha256_hex(presented);
    constant_time_eq(computed.as_bytes(), stored_hash.as_bytes())
}
