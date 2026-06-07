//! Key derivation with HKDF-SHA256.
//!
//! [HKDF] turns one piece of input keying material (IKM) — a master key, a
//! shared secret, a high-entropy token — into one or more cryptographically
//! independent subkeys. Use it to derive purpose-specific keys (e.g. one for
//! encryption, one for signing) from a single root secret instead of reusing
//! the same key everywhere.
//!
//! HKDF is **deterministic**: the same `(ikm, salt, info, out_len)` always
//! yields the same bytes, which is exactly what you want for key derivation
//! (both sides derive the same key). The two context parameters serve distinct
//! roles:
//!
//! * `salt` — a non-secret value mixed into the *extract* step. It need not be
//!   secret or random for security, but a per-tenant / per-application salt
//!   keeps derivations from different deployments independent.
//! * `info` — a context/label bound into the *expand* step (e.g. `b"aead-key"`
//!   vs `b"mac-key"`). Different `info` values from the same IKM produce
//!   unrelated keys.
//!
//! Note HKDF is **not** a password hash: it is fast and does no work-factor
//! stretching, so it is appropriate for high-entropy IKM, not for passwords.
//! For passwords use [`crate::password`] (Argon2id).
//!
//! [HKDF]: https://datatracker.ietf.org/doc/html/rfc5869
//!
//! ```
//! use klauthed_security::kdf::{derive_key, derive_key_32};
//!
//! let ikm = b"shared master secret";
//!
//! // Deterministic: same inputs -> same key.
//! let a = derive_key(ikm, b"tenant-salt", b"aead-key", 32).unwrap();
//! let b = derive_key(ikm, b"tenant-salt", b"aead-key", 32).unwrap();
//! assert_eq!(a, b);
//!
//! // Different `info` -> independent key.
//! let mac = derive_key(ikm, b"tenant-salt", b"mac-key", 32).unwrap();
//! assert_ne!(a, mac);
//!
//! // 32-byte convenience for an `EncryptionKey`.
//! let key32: [u8; 32] = derive_key_32(ikm, b"tenant-salt", b"aead-key").unwrap();
//! assert_eq!(&key32[..], &a[..]);
//! ```

use ring::hkdf::{HKDF_SHA256, KeyType, Salt};

use crate::error::SecurityError;

/// A [`KeyType`] describing an arbitrary HKDF output length in bytes.
struct OutLen(usize);

impl KeyType for OutLen {
    fn len(&self) -> usize {
        self.0
    }
}

/// Derive `out_len` bytes of keying material from `ikm` using HKDF-SHA256.
///
/// Deterministic in all four inputs. `salt` and `info` provide domain
/// separation (see the [module docs](crate::kdf)).
///
/// # Errors
///
/// Returns [`SecurityError::KeyDerivation`] if `out_len` exceeds HKDF's ceiling
/// of `255 * 32 = 8160` bytes for SHA-256.
pub fn derive_key(
    ikm: &[u8],
    salt: &[u8],
    info: &[u8],
    out_len: usize,
) -> Result<Vec<u8>, SecurityError> {
    let prk = Salt::new(HKDF_SHA256, salt).extract(ikm);
    let info = [info];
    let okm = prk.expand(&info, OutLen(out_len)).map_err(|_| SecurityError::KeyDerivation)?;
    let mut out = vec![0u8; out_len];
    okm.fill(&mut out).map_err(|_| SecurityError::KeyDerivation)?;
    Ok(out)
}

/// Derive exactly 32 bytes — e.g. for an
/// [`EncryptionKey`](crate::aead::EncryptionKey) — from `ikm`.
///
/// A thin wrapper over [`derive_key`] with `out_len = 32`.
///
/// # Errors
///
/// This cannot fail for a 32-byte output, but the signature is kept fallible
/// for consistency and future-proofing.
pub fn derive_key_32(ikm: &[u8], salt: &[u8], info: &[u8]) -> Result<[u8; 32], SecurityError> {
    let v = derive_key(ikm, salt, info, 32)?;
    let mut out = [0u8; 32];
    out.copy_from_slice(&v);
    Ok(out)
}
