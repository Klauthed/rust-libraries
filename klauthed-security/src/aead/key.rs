//! The [`EncryptionKey`] type: a zeroizing AES-256 key.

use ring::aead::{AES_256_GCM, LessSafeKey, UnboundKey};
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
        SystemRandom::new().fill(&mut bytes).map_err(|_| SecurityError::Rng)?;
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
    pub(crate) fn less_safe_key(&self) -> Result<LessSafeKey, SecurityError> {
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
