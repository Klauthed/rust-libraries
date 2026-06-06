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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_for_same_inputs() {
        let a = derive_key(b"ikm", b"salt", b"info", 32).unwrap();
        let b = derive_key(b"ikm", b"salt", b"info", 32).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.len(), 32);
    }

    #[test]
    fn different_info_diverges() {
        let a = derive_key(b"ikm", b"salt", b"info-a", 32).unwrap();
        let b = derive_key(b"ikm", b"salt", b"info-b", 32).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn different_salt_diverges() {
        let a = derive_key(b"ikm", b"salt-a", b"info", 32).unwrap();
        let b = derive_key(b"ikm", b"salt-b", b"info", 32).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn different_ikm_diverges() {
        let a = derive_key(b"ikm-a", b"salt", b"info", 32).unwrap();
        let b = derive_key(b"ikm-b", b"salt", b"info", 32).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn respects_requested_length() {
        assert_eq!(derive_key(b"ikm", b"s", b"i", 16).unwrap().len(), 16);
        assert_eq!(derive_key(b"ikm", b"s", b"i", 64).unwrap().len(), 64);
    }

    #[test]
    fn too_long_output_errors() {
        let err = derive_key(b"ikm", b"s", b"i", 255 * 32 + 1).unwrap_err();
        assert!(matches!(err, SecurityError::KeyDerivation));
    }

    #[test]
    fn derive_key_32_matches_derive_key() {
        let a = derive_key_32(b"ikm", b"salt", b"info").unwrap();
        let b = derive_key(b"ikm", b"salt", b"info", 32).unwrap();
        assert_eq!(&a[..], &b[..]);
    }

    // RFC 5869 Test Case 1 (HKDF-SHA256) known-answer.
    #[test]
    fn rfc5869_test_case_1() {
        let ikm = [0x0bu8; 22];
        let salt: Vec<u8> = (0x00u8..=0x0c).collect();
        let info: Vec<u8> = (0xf0u8..=0xf9).collect();
        let okm = derive_key(&ikm, &salt, &info, 42).unwrap();
        let expected = hex::decode(
            "3cb25f25faacd57a90434f64d0362f2a\
             2d2d0a90cf1a5a4c5db02d56ecc4c5bf\
             34007208d5b887185865",
        )
        .unwrap();
        assert_eq!(okm, expected);
    }
}
