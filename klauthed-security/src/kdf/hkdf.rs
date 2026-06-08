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
