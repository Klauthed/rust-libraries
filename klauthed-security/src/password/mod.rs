//! Argon2id password hashing.
//!
//! Hashes are produced as self-describing [PHC strings] (algorithm, version,
//! parameters and a random salt all encoded inline), so verification needs only
//! the stored string and the candidate password.
//!
//! [PHC strings]: https://github.com/P-H-C/phc-string-format/blob/master/phc-sf-spec.md
//!
//! ```
//! use klauthed_security::password::{hash_password, verify_password};
//!
//! let phc = hash_password("correct horse battery staple").unwrap();
//! assert!(verify_password("correct horse battery staple", &phc).unwrap());
//! assert!(!verify_password("Tr0ub4dour&3", &phc).unwrap());
//! ```

use argon2::Argon2;
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};

use crate::error::SecurityError;

/// Hash `password` with Argon2id and a fresh random salt, returning a PHC string.
///
/// The returned string embeds the algorithm, parameters and salt, so it is the
/// only thing that needs to be persisted to later [`verify_password`].
pub fn hash_password(password: &str) -> Result<String, SecurityError> {
    // 16 bytes of OS entropy, base64-encoded into a PHC-compatible salt.
    let salt = SaltString::generate(&mut OsRng);
    // Argon2id with library-recommended default parameters.
    let argon2 = Argon2::default();
    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|e| SecurityError::Hash(e.to_string()))
}

/// Verify `password` against a stored PHC `phc_hash`.
///
/// Returns `Ok(true)` on a match, `Ok(false)` on a (cryptographically verified)
/// mismatch, and an error only if `phc_hash` itself cannot be parsed.
pub fn verify_password(password: &str, phc_hash: &str) -> Result<bool, SecurityError> {
    let parsed =
        PasswordHash::new(phc_hash).map_err(|e| SecurityError::InvalidHash(e.to_string()))?;
    match Argon2::default().verify_password(password.as_bytes(), &parsed) {
        Ok(()) => Ok(true),
        Err(argon2::password_hash::Error::Password) => Ok(false),
        Err(e) => Err(SecurityError::Hash(e.to_string())),
    }
}
