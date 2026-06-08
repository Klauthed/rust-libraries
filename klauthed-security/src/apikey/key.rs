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
