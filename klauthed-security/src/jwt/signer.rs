//! [`JwtSigner`] — encodes [`Claims`] into compact, signed JWTs.
//!
//! Supports HMAC (HS256), RSA (RS256), ECDSA P-256 (ES256), and Ed25519
//! (EdDSA). Asymmetric keys can be loaded from PEM (config files) or DER (raw
//! key material, e.g. from a KMS/HSM).

use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};

use crate::error::SecurityError;

use super::Claims;

/// Signs [`Claims`] into compact JWTs.
pub struct JwtSigner {
    header: Header,
    key: EncodingKey,
}

impl JwtSigner {
    /// An HS256 signer using `secret` as the shared HMAC key.
    #[must_use]
    pub fn hs256(secret: &[u8]) -> Self {
        Self { header: Header::new(Algorithm::HS256), key: EncodingKey::from_secret(secret) }
    }

    /// An RS256 signer from a PKCS#1/PKCS#8 RSA **private** key in PEM form.
    ///
    /// # Errors
    /// Returns [`SecurityError::Key`] if the PEM cannot be parsed.
    pub fn rs256_pem(private_key_pem: &[u8]) -> Result<Self, SecurityError> {
        let key = EncodingKey::from_rsa_pem(private_key_pem)
            .map_err(|e| SecurityError::Key(e.to_string()))?;
        Ok(Self { header: Header::new(Algorithm::RS256), key })
    }

    /// An RS256 signer from a DER-encoded PKCS#1 RSA **private** key.
    #[must_use]
    pub fn rs256_der(private_key_der: &[u8]) -> Self {
        Self {
            header: Header::new(Algorithm::RS256),
            key: EncodingKey::from_rsa_der(private_key_der),
        }
    }

    /// An ES256 (ECDSA P-256) signer from a PKCS#8 EC **private** key in PEM form.
    ///
    /// # Errors
    /// Returns [`SecurityError::Key`] if the PEM cannot be parsed.
    pub fn es256_pem(private_key_pem: &[u8]) -> Result<Self, SecurityError> {
        let key = EncodingKey::from_ec_pem(private_key_pem)
            .map_err(|e| SecurityError::Key(e.to_string()))?;
        Ok(Self { header: Header::new(Algorithm::ES256), key })
    }

    /// An ES256 signer from a DER-encoded PKCS#8 EC **private** key.
    #[must_use]
    pub fn es256_der(private_key_der: &[u8]) -> Self {
        Self {
            header: Header::new(Algorithm::ES256),
            key: EncodingKey::from_ec_der(private_key_der),
        }
    }

    /// An EdDSA (Ed25519) signer from a PKCS#8 **private** key in PEM form.
    ///
    /// # Errors
    /// Returns [`SecurityError::Key`] if the PEM cannot be parsed.
    pub fn eddsa_pem(private_key_pem: &[u8]) -> Result<Self, SecurityError> {
        let key = EncodingKey::from_ed_pem(private_key_pem)
            .map_err(|e| SecurityError::Key(e.to_string()))?;
        Ok(Self { header: Header::new(Algorithm::EdDSA), key })
    }

    /// An EdDSA signer from a DER-encoded PKCS#8 Ed25519 **private** key.
    #[must_use]
    pub fn eddsa_der(private_key_der: &[u8]) -> Self {
        Self {
            header: Header::new(Algorithm::EdDSA),
            key: EncodingKey::from_ed_der(private_key_der),
        }
    }

    /// Encode and sign `claims`, returning the compact JWT string.
    ///
    /// # Errors
    /// Returns [`SecurityError::Encode`] if serialization/signing fails.
    pub fn encode(&self, claims: &Claims) -> Result<String, SecurityError> {
        encode(&self.header, claims, &self.key).map_err(|e| SecurityError::Encode(e.to_string()))
    }
}
