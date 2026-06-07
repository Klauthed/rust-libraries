//! Envelope encryption: per-message data keys wrapped under a long-lived root key.
//!
//! [`seal`] draws a fresh random *data key* (DEK), encrypts the payload under it,
//! and then encrypts (*wraps*) that DEK under the caller's *root key* (KEK). The
//! resulting [`Envelope`] carries the wrapped DEK alongside the DEK-encrypted
//! ciphertext. [`Envelope::open`] reverses it.
//!
//! Why bother instead of [`encrypt`] with one key?
//!
//! * **Cheap root-key rotation** — [`Envelope::rewrap`] re-wraps the small DEK
//!   under a new root key without re-encrypting the (possibly large) payload.
//! * **Blast-radius containment** — each message has its own DEK; the root key
//!   can live in a KMS/HSM and only ever wrap/unwrap 32-byte keys.
//!
//! ```
//! use klauthed_security::aead::{EncryptionKey, Envelope, seal};
//!
//! let root = EncryptionKey::generate().unwrap();
//! let sealed = seal(&root, b"card number", b"user:42").unwrap();
//!
//! // Persist a self-contained frame, restore, and open.
//! let restored = Envelope::from_bytes(&sealed.to_bytes()).unwrap();
//! assert_eq!(restored.open(&root, b"user:42").unwrap(), b"card number");
//!
//! // Rotate the root key without touching the ciphertext.
//! let new_root = EncryptionKey::generate().unwrap();
//! let rotated = sealed.rewrap(&root, &new_root).unwrap();
//! assert_eq!(rotated.open(&new_root, b"user:42").unwrap(), b"card number");
//! ```

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;

use super::{EncryptionKey, KEY_LEN, decrypt, encrypt};
use crate::error::SecurityError;

/// AAD that binds a wrapped data key to this envelope scheme and version, so a
/// wrapped key cannot be repurposed as a normal ciphertext (or vice versa).
const WRAP_AAD: &[u8] = b"klauthed.aead.envelope.v1";

/// A sealed message: a data key wrapped under a root key, plus the payload
/// encrypted under that data key.
///
/// Both halves are independent AEAD ciphertexts (`nonce ‖ ciphertext ‖ tag`).
/// Serialize with [`to_bytes`](Envelope::to_bytes) / [`to_base64`](Envelope::to_base64).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    wrapped_key: Vec<u8>,
    ciphertext: Vec<u8>,
}

/// Seal `plaintext` under a fresh data key, wrapping that data key under
/// `root_key`. `aad` authenticates the payload exactly as in
/// [`encrypt`] and must be supplied again to
/// [`open`](Envelope::open).
///
/// # Errors
/// Returns [`SecurityError::Rng`] if a data key/nonce can't be generated, or
/// [`SecurityError::Encryption`] if sealing fails.
pub fn seal(
    root_key: &EncryptionKey,
    plaintext: &[u8],
    aad: &[u8],
) -> Result<Envelope, SecurityError> {
    let dek = EncryptionKey::generate()?;
    let ciphertext = encrypt(&dek, plaintext, aad)?;
    let wrapped_key = encrypt(root_key, dek.as_bytes(), WRAP_AAD)?;
    Ok(Envelope { wrapped_key, ciphertext })
}

impl Envelope {
    /// Borrow the wrapped (root-key-encrypted) data key.
    #[must_use]
    pub fn wrapped_key(&self) -> &[u8] {
        &self.wrapped_key
    }

    /// Borrow the data-key-encrypted payload.
    #[must_use]
    pub fn ciphertext(&self) -> &[u8] {
        &self.ciphertext
    }

    /// Unwrap the data key with `root_key`.
    fn unwrap_dek(&self, root_key: &EncryptionKey) -> Result<EncryptionKey, SecurityError> {
        let raw = decrypt(root_key, &self.wrapped_key, WRAP_AAD)?;
        let bytes: [u8; KEY_LEN] = raw.try_into().map_err(|_| SecurityError::Decryption)?;
        Ok(EncryptionKey::from_bytes(bytes))
    }

    /// Unwrap the data key with `root_key` and decrypt the payload.
    ///
    /// # Errors
    /// Returns [`SecurityError::Decryption`] if `root_key`/`aad` are wrong or
    /// either layer has been tampered with.
    pub fn open(&self, root_key: &EncryptionKey, aad: &[u8]) -> Result<Vec<u8>, SecurityError> {
        let dek = self.unwrap_dek(root_key)?;
        decrypt(&dek, &self.ciphertext, aad)
    }

    /// Re-wrap the data key from `current_root` under `new_root`, leaving the
    /// payload ciphertext untouched — the cheap half of root-key rotation.
    ///
    /// # Errors
    /// Returns [`SecurityError::Decryption`] if `current_root` cannot unwrap the
    /// data key, or [`SecurityError::Encryption`] if re-wrapping fails.
    pub fn rewrap(
        &self,
        current_root: &EncryptionKey,
        new_root: &EncryptionKey,
    ) -> Result<Envelope, SecurityError> {
        let dek = self.unwrap_dek(current_root)?;
        let wrapped_key = encrypt(new_root, dek.as_bytes(), WRAP_AAD)?;
        Ok(Envelope { wrapped_key, ciphertext: self.ciphertext.clone() })
    }

    /// Serialize to a self-contained frame:
    /// `len(wrapped_key) as u32 BE ‖ wrapped_key ‖ ciphertext`.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(4 + self.wrapped_key.len() + self.ciphertext.len());
        out.extend_from_slice(&(self.wrapped_key.len() as u32).to_be_bytes());
        out.extend_from_slice(&self.wrapped_key);
        out.extend_from_slice(&self.ciphertext);
        out
    }

    /// Parse the frame produced by [`to_bytes`](Envelope::to_bytes).
    ///
    /// # Errors
    /// Returns [`SecurityError::Decryption`] if the frame is truncated or its
    /// length prefix is inconsistent.
    pub fn from_bytes(bytes: &[u8]) -> Result<Envelope, SecurityError> {
        let (len_be, rest) = bytes.split_first_chunk::<4>().ok_or(SecurityError::Decryption)?;
        let wrapped_len = u32::from_be_bytes(*len_be) as usize;
        if rest.len() < wrapped_len {
            return Err(SecurityError::Decryption);
        }
        let (wrapped_key, ciphertext) = rest.split_at(wrapped_len);
        Ok(Envelope { wrapped_key: wrapped_key.to_vec(), ciphertext: ciphertext.to_vec() })
    }

    /// Standard base64 of [`to_bytes`](Envelope::to_bytes) — convenient for a
    /// text column or JSON field.
    #[must_use]
    pub fn to_base64(&self) -> String {
        BASE64.encode(self.to_bytes())
    }

    /// Parse the base64 produced by [`to_base64`](Envelope::to_base64).
    ///
    /// # Errors
    /// Returns [`SecurityError::Decryption`] if the input is not valid base64 or
    /// not a valid envelope frame.
    pub fn from_base64(s: &str) -> Result<Envelope, SecurityError> {
        let raw = BASE64.decode(s).map_err(|_| SecurityError::Decryption)?;
        Envelope::from_bytes(&raw)
    }
}
