//! The AEAD sealing/opening free functions over an [`EncryptionKey`].

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use ring::aead::{Aad, NONCE_LEN, Nonce};
use ring::rand::{SecureRandom, SystemRandom};

use super::EncryptionKey;
use crate::error::SecurityError;

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
pub fn encrypt(
    key: &EncryptionKey,
    plaintext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, SecurityError> {
    let sealing = key.less_safe_key()?;

    let mut nonce_bytes = [0u8; NONCE_LEN];
    SystemRandom::new().fill(&mut nonce_bytes).map_err(|_| SecurityError::Rng)?;
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
    let nonce =
        Nonce::try_assume_unique_for_key(nonce_bytes).map_err(|_| SecurityError::Decryption)?;

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
    let raw = BASE64.decode(ciphertext_b64).map_err(|_| SecurityError::Decryption)?;
    decrypt(key, &raw, aad)
}

#[cfg(test)]
mod proptests {
    use super::{EncryptionKey, decrypt, decrypt_from_base64, encrypt, encrypt_to_base64};
    use proptest::prelude::*;

    fn bytes() -> impl Strategy<Value = Vec<u8>> {
        prop::collection::vec(any::<u8>(), 0..256)
    }

    proptest! {
        // Encrypt then decrypt under the same key + AAD returns the plaintext.
        #[test]
        fn encrypt_decrypt_round_trips(
            key_bytes in any::<[u8; 32]>(),
            plaintext in bytes(),
            aad in bytes(),
        ) {
            let key = EncryptionKey::from_bytes(key_bytes);
            let ciphertext = encrypt(&key, &plaintext, &aad).unwrap();
            prop_assert_eq!(decrypt(&key, &ciphertext, &aad).unwrap(), plaintext);
        }

        // The base64 helpers round-trip too.
        #[test]
        fn base64_round_trips(
            key_bytes in any::<[u8; 32]>(),
            plaintext in bytes(),
            aad in bytes(),
        ) {
            let key = EncryptionKey::from_bytes(key_bytes);
            let ciphertext = encrypt_to_base64(&key, &plaintext, &aad).unwrap();
            prop_assert_eq!(decrypt_from_base64(&key, &ciphertext, &aad).unwrap(), plaintext);
        }

        // A different key fails the GCM authentication tag.
        #[test]
        fn a_different_key_fails_authentication(
            key_a in any::<[u8; 32]>(),
            key_b in any::<[u8; 32]>(),
            plaintext in bytes(),
            aad in bytes(),
        ) {
            prop_assume!(key_a != key_b);
            let ciphertext = encrypt(&EncryptionKey::from_bytes(key_a), &plaintext, &aad).unwrap();
            prop_assert!(decrypt(&EncryptionKey::from_bytes(key_b), &ciphertext, &aad).is_err());
        }

        // A different AAD fails authentication (the AAD is bound to the ciphertext).
        #[test]
        fn a_different_aad_fails_authentication(
            key_bytes in any::<[u8; 32]>(),
            plaintext in bytes(),
            aad_a in bytes(),
            aad_b in bytes(),
        ) {
            prop_assume!(aad_a != aad_b);
            let key = EncryptionKey::from_bytes(key_bytes);
            let ciphertext = encrypt(&key, &plaintext, &aad_a).unwrap();
            prop_assert!(decrypt(&key, &ciphertext, &aad_b).is_err());
        }
    }
}
