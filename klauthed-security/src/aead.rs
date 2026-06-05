//! Authenticated symmetric encryption (AES-256-GCM).
//!
//! [AEAD] gives you both confidentiality *and* integrity: a single key both
//! encrypts the plaintext and produces an authentication tag, so any tampering
//! with the ciphertext — or the use of a wrong key or wrong associated data —
//! is detected as a failed decryption rather than silently returning garbage.
//!
//! This module wraps `ring`'s AES-256-GCM. The wire format is deliberately
//! self-contained:
//!
//! ```text
//! output = nonce (12 bytes) || ciphertext || GCM tag (16 bytes)
//! ```
//!
//! A fresh 12-byte nonce is drawn from the OS CSPRNG for **every** call to
//! [`encrypt`] and prepended to the output, so callers never have to manage
//! nonces themselves and nonces are never reused under a given key. (GCM is
//! catastrophically broken if a nonce is reused with the same key; generating a
//! random nonce per message is the supported, misuse-resistant pattern here.)
//!
//! The *associated data* (AAD) is authenticated but not encrypted: bind context
//! such as a record id, a tenant id, or a version tag to the ciphertext so it
//! cannot be replayed in a different context. The exact same AAD must be passed
//! to [`decrypt`].
//!
//! [AEAD]: https://en.wikipedia.org/wiki/Authenticated_encryption
//!
//! ```
//! use klauthed_security::aead::{encrypt, decrypt, EncryptionKey};
//!
//! let key = EncryptionKey::generate().unwrap();
//! let aad = b"record:42";
//!
//! let sealed = encrypt(&key, b"top secret", aad).unwrap();
//! let opened = decrypt(&key, &sealed, aad).unwrap();
//! assert_eq!(opened, b"top secret");
//!
//! // Wrong AAD is rejected by the tag check.
//! assert!(decrypt(&key, &sealed, b"record:99").is_err());
//! ```
//!
//! # Future work
//!
//! Envelope encryption / KMS integration (wrapping these data keys under a
//! root key) and asymmetric encryption are intentionally out of scope.

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM, NONCE_LEN};
use ring::rand::{SecureRandom, SystemRandom};
use zeroize::Zeroize;

use crate::error::SecurityError;

/// The length of an AES-256 key, in bytes.
pub const KEY_LEN: usize = 32;

/// A 256-bit secret key for AES-256-GCM.
///
/// The raw key bytes are zeroized on drop. Construct one from existing bytes
/// with [`EncryptionKey::from_bytes`] (e.g. material loaded from a secret store
/// or derived via [`crate::kdf::derive_key`]) or draw a fresh random key with
/// [`EncryptionKey::generate`].
pub struct EncryptionKey {
    bytes: [u8; KEY_LEN],
}

impl EncryptionKey {
    /// Build a key from exactly [`KEY_LEN`] (32) raw bytes.
    ///
    /// ```
    /// use klauthed_security::aead::{EncryptionKey, KEY_LEN};
    ///
    /// let key = EncryptionKey::from_bytes([7u8; KEY_LEN]);
    /// # let _ = key;
    /// ```
    #[must_use]
    pub fn from_bytes(bytes: [u8; KEY_LEN]) -> Self {
        Self { bytes }
    }

    /// Draw a fresh random 256-bit key from the OS CSPRNG.
    ///
    /// # Errors
    ///
    /// Returns [`SecurityError::Rng`] if the OS CSPRNG fails.
    pub fn generate() -> Result<Self, SecurityError> {
        let mut bytes = [0u8; KEY_LEN];
        SystemRandom::new()
            .fill(&mut bytes)
            .map_err(|_| SecurityError::Rng)?;
        Ok(Self { bytes })
    }

    /// Borrow the raw key bytes.
    ///
    /// Prefer keeping the key opaque; this exists for persisting/wrapping the
    /// key material. Avoid copying it into long-lived, un-zeroized buffers.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.bytes
    }

    /// Construct the `ring` sealing/opening key for this key.
    fn less_safe_key(&self) -> Result<LessSafeKey, SecurityError> {
        let unbound = UnboundKey::new(&AES_256_GCM, &self.bytes)
            // The key length is fixed at construction, so this cannot realistically
            // fail; map it to an internal encryption fault rather than panic.
            .map_err(|_| SecurityError::Encryption)?;
        Ok(LessSafeKey::new(unbound))
    }
}

impl Drop for EncryptionKey {
    fn drop(&mut self) {
        self.bytes.zeroize();
    }
}

impl std::fmt::Debug for EncryptionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print key material.
        f.debug_struct("EncryptionKey").finish_non_exhaustive()
    }
}

/// Encrypt `plaintext` under `key`, authenticating `aad`.
///
/// A fresh random 12-byte nonce is generated and prepended to the result:
/// `nonce || ciphertext || tag`. The same `aad` must be supplied to
/// [`decrypt`].
///
/// # Errors
///
/// Returns [`SecurityError::Rng`] if the nonce cannot be generated, or
/// [`SecurityError::Encryption`] if sealing fails.
pub fn encrypt(key: &EncryptionKey, plaintext: &[u8], aad: &[u8]) -> Result<Vec<u8>, SecurityError> {
    let sealing = key.less_safe_key()?;

    let mut nonce_bytes = [0u8; NONCE_LEN];
    SystemRandom::new()
        .fill(&mut nonce_bytes)
        .map_err(|_| SecurityError::Rng)?;
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);

    // ring seals in place and appends the tag, so start the buffer with the
    // plaintext and let it grow by TAG_LEN.
    let mut in_out = plaintext.to_vec();
    sealing
        .seal_in_place_append_tag(nonce, Aad::from(aad), &mut in_out)
        .map_err(|_| SecurityError::Encryption)?;

    // Prepend the nonce: output = nonce || ciphertext || tag.
    let mut out = Vec::with_capacity(NONCE_LEN + in_out.len());
    out.extend_from_slice(&nonce_bytes);
    out.append(&mut in_out);
    Ok(out)
}

/// Decrypt a `nonce || ciphertext || tag` blob produced by [`encrypt`].
///
/// `aad` must match the value used at encryption time. Any tampering with the
/// ciphertext, a wrong key, or a wrong `aad` fails the GCM tag check.
///
/// # Errors
///
/// Returns [`SecurityError::Decryption`] if the input is too short to contain a
/// nonce + tag, or if authentication fails (tampered ciphertext, wrong key, or
/// wrong AAD).
pub fn decrypt(
    key: &EncryptionKey,
    ciphertext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, SecurityError> {
    if ciphertext.len() < NONCE_LEN {
        return Err(SecurityError::Decryption);
    }
    let (nonce_bytes, sealed) = ciphertext.split_at(NONCE_LEN);
    let nonce = Nonce::try_assume_unique_for_key(nonce_bytes)
        .map_err(|_| SecurityError::Decryption)?;

    let opening = key.less_safe_key()?;
    let mut in_out = sealed.to_vec();
    let plaintext = opening
        .open_in_place(nonce, Aad::from(aad), &mut in_out)
        .map_err(|_| SecurityError::Decryption)?;
    Ok(plaintext.to_vec())
}

/// Like [`encrypt`], but returns standard base64 of `nonce || ciphertext || tag`.
///
/// Convenient for storing ciphertext in a text column or JSON field.
///
/// # Errors
///
/// As [`encrypt`].
pub fn encrypt_to_base64(
    key: &EncryptionKey,
    plaintext: &[u8],
    aad: &[u8],
) -> Result<String, SecurityError> {
    Ok(BASE64.encode(encrypt(key, plaintext, aad)?))
}

/// Like [`decrypt`], but accepts the standard base64 produced by
/// [`encrypt_to_base64`].
///
/// # Errors
///
/// Returns [`SecurityError::Decryption`] if the input is not valid base64, in
/// addition to the failure modes of [`decrypt`].
pub fn decrypt_from_base64(
    key: &EncryptionKey,
    ciphertext_b64: &str,
    aad: &[u8],
) -> Result<Vec<u8>, SecurityError> {
    let raw = BASE64
        .decode(ciphertext_b64)
        .map_err(|_| SecurityError::Decryption)?;
    decrypt(key, &raw, aad)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::aead::NONCE_LEN;

    #[test]
    fn round_trip_recovers_plaintext() {
        let key = EncryptionKey::generate().unwrap();
        let pt = b"the quick brown fox";
        let aad = b"ctx:1";
        let ct = encrypt(&key, pt, aad).unwrap();
        // Output carries the prepended nonce and the 16-byte GCM tag.
        assert_eq!(ct.len(), NONCE_LEN + pt.len() + 16);
        assert_eq!(decrypt(&key, &ct, aad).unwrap(), pt);
    }

    #[test]
    fn empty_plaintext_round_trips() {
        let key = EncryptionKey::generate().unwrap();
        let ct = encrypt(&key, b"", b"").unwrap();
        assert_eq!(decrypt(&key, &ct, b"").unwrap(), b"");
    }

    #[test]
    fn nonce_is_random_per_encryption() {
        let key = EncryptionKey::generate().unwrap();
        let a = encrypt(&key, b"same plaintext", b"aad").unwrap();
        let b = encrypt(&key, b"same plaintext", b"aad").unwrap();
        assert_ne!(a, b, "fresh nonce must make ciphertexts differ");
        assert_eq!(decrypt(&key, &a, b"aad").unwrap(), b"same plaintext");
        assert_eq!(decrypt(&key, &b, b"aad").unwrap(), b"same plaintext");
    }

    #[test]
    fn wrong_key_fails() {
        let key = EncryptionKey::generate().unwrap();
        let other = EncryptionKey::generate().unwrap();
        let ct = encrypt(&key, b"secret", b"aad").unwrap();
        let err = decrypt(&other, &ct, b"aad").unwrap_err();
        assert!(matches!(err, SecurityError::Decryption));
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let key = EncryptionKey::generate().unwrap();
        let mut ct = encrypt(&key, b"secret", b"aad").unwrap();
        // Flip a bit in the ciphertext body (past the nonce).
        let i = NONCE_LEN + 1;
        ct[i] ^= 0x01;
        assert!(matches!(
            decrypt(&key, &ct, b"aad").unwrap_err(),
            SecurityError::Decryption
        ));
    }

    #[test]
    fn tampered_tag_fails() {
        let key = EncryptionKey::generate().unwrap();
        let mut ct = encrypt(&key, b"secret", b"aad").unwrap();
        let last = ct.len() - 1;
        ct[last] ^= 0x80;
        assert!(matches!(
            decrypt(&key, &ct, b"aad").unwrap_err(),
            SecurityError::Decryption
        ));
    }

    #[test]
    fn wrong_aad_fails() {
        let key = EncryptionKey::generate().unwrap();
        let ct = encrypt(&key, b"secret", b"record:1").unwrap();
        assert!(matches!(
            decrypt(&key, &ct, b"record:2").unwrap_err(),
            SecurityError::Decryption
        ));
    }

    #[test]
    fn too_short_input_fails() {
        let key = EncryptionKey::generate().unwrap();
        assert!(matches!(
            decrypt(&key, &[0u8; NONCE_LEN], b"").unwrap_err(),
            SecurityError::Decryption
        ));
        assert!(matches!(
            decrypt(&key, b"short", b"").unwrap_err(),
            SecurityError::Decryption
        ));
    }

    #[test]
    fn base64_round_trip() {
        let key = EncryptionKey::generate().unwrap();
        let s = encrypt_to_base64(&key, b"hello", b"v1").unwrap();
        assert_eq!(decrypt_from_base64(&key, &s, b"v1").unwrap(), b"hello");
        assert!(decrypt_from_base64(&key, "not valid base64!!", b"v1").is_err());
    }

    #[test]
    fn from_bytes_is_deterministic_key() {
        let key = EncryptionKey::from_bytes([42u8; KEY_LEN]);
        let ct = encrypt(&key, b"data", b"").unwrap();
        // A separate key built from identical bytes can decrypt.
        let same = EncryptionKey::from_bytes([42u8; KEY_LEN]);
        assert_eq!(decrypt(&same, &ct, b"").unwrap(), b"data");
    }

    #[test]
    fn debug_does_not_leak_key() {
        let key = EncryptionKey::from_bytes([0xABu8; KEY_LEN]);
        let s = format!("{key:?}");
        assert!(!s.contains("AB") && !s.contains("171"));
    }
}
