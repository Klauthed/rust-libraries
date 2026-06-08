//! Public-API integration tests for sealed-box (public-key) encryption.
#![cfg(feature = "sealed")]

use klauthed_security::SecurityError;
use klauthed_security::aead::asymmetric::{KeyPair, PublicKey, SecretKey, open, seal_to};

#[test]
fn seal_open_round_trip() {
    let recipient = KeyPair::generate().unwrap();
    let sealed = seal_to(recipient.public(), b"top secret", b"ctx:1").unwrap();
    assert_eq!(open(recipient.secret(), &sealed, b"ctx:1").unwrap(), b"top secret");
}

#[test]
fn each_seal_is_unique() {
    let recipient = KeyPair::generate().unwrap();
    let a = seal_to(recipient.public(), b"same", b"aad").unwrap();
    let b = seal_to(recipient.public(), b"same", b"aad").unwrap();
    // Fresh ephemeral key per message => different ciphertext (and prefix).
    assert_ne!(a, b);
    assert_eq!(open(recipient.secret(), &a, b"aad").unwrap(), b"same");
    assert_eq!(open(recipient.secret(), &b, b"aad").unwrap(), b"same");
}

#[test]
fn wrong_recipient_cannot_open() {
    let recipient = KeyPair::generate().unwrap();
    let other = KeyPair::generate().unwrap();
    let sealed = seal_to(recipient.public(), b"secret", b"aad").unwrap();
    assert!(matches!(
        open(other.secret(), &sealed, b"aad").unwrap_err(),
        SecurityError::Decryption
    ));
}

#[test]
fn wrong_aad_fails() {
    let recipient = KeyPair::generate().unwrap();
    let sealed = seal_to(recipient.public(), b"secret", b"record:1").unwrap();
    assert!(matches!(
        open(recipient.secret(), &sealed, b"record:2").unwrap_err(),
        SecurityError::Decryption
    ));
}

#[test]
fn tampered_ciphertext_fails() {
    let recipient = KeyPair::generate().unwrap();
    let mut sealed = seal_to(recipient.public(), b"secret", b"aad").unwrap();
    let last = sealed.len() - 1;
    sealed[last] ^= 0x01;
    assert!(matches!(
        open(recipient.secret(), &sealed, b"aad").unwrap_err(),
        SecurityError::Decryption
    ));
}

#[test]
fn truncated_input_fails() {
    let recipient = KeyPair::generate().unwrap();
    // Shorter than the 32-byte ephemeral public key prefix.
    assert!(matches!(
        open(recipient.secret(), &[0u8; 8], b"").unwrap_err(),
        SecurityError::Decryption
    ));
}

#[test]
fn keys_round_trip_through_bytes() {
    let kp = KeyPair::generate().unwrap();
    let sealed = seal_to(kp.public(), b"data", b"v1").unwrap();

    // Restore the secret key from its bytes and open.
    let restored = SecretKey::from_bytes(kp.secret().to_bytes());
    assert_eq!(open(&restored, &sealed, b"v1").unwrap(), b"data");

    // Restore the public key from bytes and seal to it.
    let pub_restored = PublicKey::from_bytes(kp.public().to_bytes());
    let sealed2 = seal_to(&pub_restored, b"data2", b"v1").unwrap();
    assert_eq!(open(kp.secret(), &sealed2, b"v1").unwrap(), b"data2");

    // Public key derived from the secret matches.
    assert_eq!(restored.public_key().to_bytes(), kp.public().to_bytes());
}

#[test]
fn secret_key_debug_does_not_leak() {
    let kp = KeyPair::generate().unwrap();
    assert_eq!(format!("{:?}", kp.secret()), "SecretKey(***)");
}
