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

pub mod cipher;
pub mod key;

pub use cipher::{decrypt, decrypt_from_base64, encrypt, encrypt_to_base64};
pub use key::{EncryptionKey, KEY_LEN};
