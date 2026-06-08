//! Sealed-box (public-key) encryption (feature `sealed`).
//!
//! [`seal_to`] lets anyone encrypt a message to a recipient's X25519
//! [`PublicKey`] **without a pre-shared key**; only the holder of the matching
//! [`SecretKey`] can [`open`] it. It is an ECIES-style sealed box:
//!
//! 1. a fresh **ephemeral** X25519 key pair is generated per message;
//! 2. X25519 Diffie–Hellman between the ephemeral secret and the recipient's
//!    public key yields a shared secret;
//! 3. that secret is run through HKDF (bound to both public keys) to derive a
//!    one-time AES-256-GCM key;
//! 4. the payload is sealed with that key (reusing [`super::encrypt`]).
//!
//! The output is `ephemeral_public_key (32 bytes) ‖ AEAD blob`. The sender is
//! anonymous and forward-secret per message (the ephemeral secret is discarded).
//!
//! ```
//! use klauthed_security::aead::asymmetric::{KeyPair, open, seal_to};
//!
//! let recipient = KeyPair::generate().unwrap();
//!
//! // Anyone with the public key can seal to the recipient.
//! let sealed = seal_to(recipient.public(), b"for your eyes only", b"ctx:1").unwrap();
//!
//! // Only the secret key opens it.
//! let opened = open(recipient.secret(), &sealed, b"ctx:1").unwrap();
//! assert_eq!(opened, b"for your eyes only");
//!
//! // A different recipient cannot.
//! let other = KeyPair::generate().unwrap();
//! assert!(open(other.secret(), &sealed, b"ctx:1").is_err());
//! ```

use ring::rand::{SecureRandom, SystemRandom};
use x25519_dalek::{PublicKey as XPublicKey, StaticSecret};

use super::{EncryptionKey, KEY_LEN, decrypt, encrypt};
use crate::error::SecurityError;
use crate::kdf::derive_key_32;

/// HKDF `info` binding a derived key to this sealed-box scheme and version.
const SEALED_INFO: &[u8] = b"klauthed.aead.sealed.v1";

/// An X25519 public key — a recipient identity to [`seal_to`].
#[derive(Debug, Clone)]
pub struct PublicKey(XPublicKey);

impl PublicKey {
    /// Build a public key from its 32 raw bytes.
    #[must_use]
    pub fn from_bytes(bytes: [u8; KEY_LEN]) -> Self {
        Self(XPublicKey::from(bytes))
    }

    /// The 32 raw bytes of the public key (safe to share).
    #[must_use]
    pub fn to_bytes(&self) -> [u8; KEY_LEN] {
        *self.0.as_bytes()
    }
}

/// An X25519 secret key. The raw scalar is zeroized on drop.
pub struct SecretKey(StaticSecret);

impl SecretKey {
    /// Draw a fresh random secret key from the OS CSPRNG.
    ///
    /// # Errors
    /// Returns [`SecurityError::Rng`] if the OS CSPRNG fails.
    pub fn generate() -> Result<Self, SecurityError> {
        let mut bytes = [0u8; KEY_LEN];
        SystemRandom::new().fill(&mut bytes).map_err(|_| SecurityError::Rng)?;
        Ok(Self(StaticSecret::from(bytes)))
    }

    /// Restore a secret key from its 32 raw bytes (e.g. loaded from a secret store).
    #[must_use]
    pub fn from_bytes(bytes: [u8; KEY_LEN]) -> Self {
        Self(StaticSecret::from(bytes))
    }

    /// The 32 raw bytes of the secret key. Handle as sensitive key material.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; KEY_LEN] {
        self.0.to_bytes()
    }

    /// The corresponding [`PublicKey`].
    #[must_use]
    pub fn public_key(&self) -> PublicKey {
        PublicKey(XPublicKey::from(&self.0))
    }
}

impl std::fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print the secret scalar.
        f.write_str("SecretKey(***)")
    }
}

/// An X25519 key pair (a [`SecretKey`] and its [`PublicKey`]).
#[derive(Debug)]
pub struct KeyPair {
    secret: SecretKey,
    public: PublicKey,
}

impl KeyPair {
    /// Generate a fresh key pair.
    ///
    /// # Errors
    /// Returns [`SecurityError::Rng`] if the OS CSPRNG fails.
    pub fn generate() -> Result<Self, SecurityError> {
        let secret = SecretKey::generate()?;
        let public = secret.public_key();
        Ok(Self { secret, public })
    }

    /// The public key (share this so others can [`seal_to`] you).
    #[must_use]
    pub fn public(&self) -> &PublicKey {
        &self.public
    }

    /// The secret key (keep this private; use it to [`open`]).
    #[must_use]
    pub fn secret(&self) -> &SecretKey {
        &self.secret
    }
}

/// Derive the one-time AEAD key from the DH shared secret, bound to both public
/// keys (standard ECIES KDF input).
fn derive_key(
    shared: &[u8],
    ephemeral_pub: &[u8; KEY_LEN],
    recipient_pub: &[u8; KEY_LEN],
) -> Result<EncryptionKey, SecurityError> {
    let mut salt = [0u8; KEY_LEN * 2];
    salt[..KEY_LEN].copy_from_slice(ephemeral_pub);
    salt[KEY_LEN..].copy_from_slice(recipient_pub);
    Ok(EncryptionKey::from_bytes(derive_key_32(shared, &salt, SEALED_INFO)?))
}

/// Seal `plaintext` to `recipient`'s public key, authenticating `aad`.
///
/// The output is `ephemeral_public_key ‖ AEAD blob` and can only be opened by
/// the matching [`SecretKey`]. `aad` must be supplied again to [`open`].
///
/// # Errors
/// Returns [`SecurityError::Rng`] if key/nonce generation fails, or
/// [`SecurityError::Encryption`] if sealing fails.
pub fn seal_to(
    recipient: &PublicKey,
    plaintext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, SecurityError> {
    let mut ephemeral_bytes = [0u8; KEY_LEN];
    SystemRandom::new().fill(&mut ephemeral_bytes).map_err(|_| SecurityError::Rng)?;
    let ephemeral_secret = StaticSecret::from(ephemeral_bytes);
    let ephemeral_pub = XPublicKey::from(&ephemeral_secret);

    let shared = ephemeral_secret.diffie_hellman(&recipient.0);
    let key = derive_key(shared.as_bytes(), ephemeral_pub.as_bytes(), recipient.0.as_bytes())?;
    let blob = encrypt(&key, plaintext, aad)?;

    let mut out = Vec::with_capacity(KEY_LEN + blob.len());
    out.extend_from_slice(ephemeral_pub.as_bytes());
    out.extend_from_slice(&blob);
    Ok(out)
}

/// Open a sealed box with the recipient's secret key.
///
/// `aad` must match the value passed to [`seal_to`].
///
/// # Errors
/// Returns [`SecurityError::Decryption`] if the input is malformed, the secret
/// key is wrong, or the `aad`/ciphertext has been tampered with.
pub fn open(recipient: &SecretKey, sealed: &[u8], aad: &[u8]) -> Result<Vec<u8>, SecurityError> {
    let (ephemeral_pub_bytes, blob) =
        sealed.split_first_chunk::<KEY_LEN>().ok_or(SecurityError::Decryption)?;
    let ephemeral_pub = XPublicKey::from(*ephemeral_pub_bytes);
    let recipient_pub = XPublicKey::from(&recipient.0);

    let shared = recipient.0.diffie_hellman(&ephemeral_pub);
    let key = derive_key(shared.as_bytes(), ephemeral_pub.as_bytes(), recipient_pub.as_bytes())?;
    decrypt(&key, blob, aad)
}
