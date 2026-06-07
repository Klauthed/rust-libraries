//! The [`TotpSecret`] base32 shared-secret type.

use totp_rs::Secret;

/// A TOTP shared secret, held as its base32 (RFC 4648, unpadded) string form —
/// the representation users see and authenticator apps consume.
#[derive(Clone, PartialEq, Eq)]
pub struct TotpSecret(String);

impl TotpSecret {
    /// Generate a fresh 160-bit secret from a CSPRNG (RFC 4226's recommended
    /// length), stored as base32.
    #[must_use]
    pub fn generate() -> Self {
        // `generate_secret` returns a `Secret::Raw`; re-encode to base32 text.
        Self(Secret::generate_secret().to_encoded().to_string())
    }

    /// Restore a secret from its stored base32 string. The value is not decoded
    /// here; an invalid base32 string surfaces later as
    /// [`SecurityError::MfaConfig`](crate::error::SecurityError::MfaConfig) when building a [`Totp`](super::Totp).
    pub fn from_base32(secret: impl Into<String>) -> Self {
        Self(secret.into())
    }

    /// The base32 secret string (e.g. to persist, or show for manual entry).
    #[must_use]
    pub fn as_base32(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for TotpSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print the secret material.
        f.write_str("TotpSecret(***)")
    }
}
