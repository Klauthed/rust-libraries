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

mod hkdf;

pub use hkdf::*;
