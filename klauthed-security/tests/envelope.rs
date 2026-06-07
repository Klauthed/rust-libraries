//! Public-API integration tests for AEAD envelope encryption: round-trips,
//! tamper/wrong-key/wrong-AAD rejection, root-key rotation, and the wire format.

use klauthed_security::SecurityError;
use klauthed_security::aead::{EncryptionKey, Envelope, seal};

#[test]
fn seal_open_round_trip() {
    let root = EncryptionKey::generate().unwrap();
    let env = seal(&root, b"top secret", b"ctx:1").unwrap();
    assert_eq!(env.open(&root, b"ctx:1").unwrap(), b"top secret");
}

#[test]
fn each_seal_uses_a_fresh_data_key() {
    let root = EncryptionKey::generate().unwrap();
    let a = seal(&root, b"same", b"aad").unwrap();
    let b = seal(&root, b"same", b"aad").unwrap();
    // Fresh DEK + fresh nonces => both halves differ across seals.
    assert_ne!(a.wrapped_key(), b.wrapped_key());
    assert_ne!(a.ciphertext(), b.ciphertext());
    assert_eq!(a.open(&root, b"aad").unwrap(), b"same");
    assert_eq!(b.open(&root, b"aad").unwrap(), b"same");
}

#[test]
fn wrong_root_key_fails() {
    let root = EncryptionKey::generate().unwrap();
    let other = EncryptionKey::generate().unwrap();
    let env = seal(&root, b"secret", b"aad").unwrap();
    assert!(matches!(env.open(&other, b"aad").unwrap_err(), SecurityError::Decryption));
}

#[test]
fn wrong_aad_fails() {
    let root = EncryptionKey::generate().unwrap();
    let env = seal(&root, b"secret", b"record:1").unwrap();
    assert!(matches!(env.open(&root, b"record:2").unwrap_err(), SecurityError::Decryption));
}

#[test]
fn tampered_ciphertext_fails() {
    let root = EncryptionKey::generate().unwrap();
    let env = seal(&root, b"secret", b"aad").unwrap();
    let mut raw = env.to_bytes();
    let last = raw.len() - 1;
    raw[last] ^= 0x01;
    let tampered = Envelope::from_bytes(&raw).unwrap();
    assert!(matches!(tampered.open(&root, b"aad").unwrap_err(), SecurityError::Decryption));
}

#[test]
fn rewrap_rotates_root_key_without_reencrypting_payload() {
    let old_root = EncryptionKey::generate().unwrap();
    let new_root = EncryptionKey::generate().unwrap();

    let env = seal(&old_root, b"rotate me", b"aad").unwrap();
    let rotated = env.rewrap(&old_root, &new_root).unwrap();

    // The payload ciphertext is unchanged; only the wrapped key differs.
    assert_eq!(rotated.ciphertext(), env.ciphertext());
    assert_ne!(rotated.wrapped_key(), env.wrapped_key());

    // The new root opens it; the old root no longer does.
    assert_eq!(rotated.open(&new_root, b"aad").unwrap(), b"rotate me");
    assert!(matches!(rotated.open(&old_root, b"aad").unwrap_err(), SecurityError::Decryption));
}

#[test]
fn rewrap_with_wrong_current_root_fails() {
    let root = EncryptionKey::generate().unwrap();
    let wrong = EncryptionKey::generate().unwrap();
    let new_root = EncryptionKey::generate().unwrap();
    let env = seal(&root, b"x", b"aad").unwrap();
    assert!(matches!(env.rewrap(&wrong, &new_root).unwrap_err(), SecurityError::Decryption));
}

#[test]
fn bytes_and_base64_round_trip() {
    let root = EncryptionKey::generate().unwrap();
    let env = seal(&root, b"hello", b"v1").unwrap();

    let from_bytes = Envelope::from_bytes(&env.to_bytes()).unwrap();
    assert_eq!(from_bytes, env);

    let from_b64 = Envelope::from_base64(&env.to_base64()).unwrap();
    assert_eq!(from_b64, env);
    assert_eq!(from_b64.open(&root, b"v1").unwrap(), b"hello");
}

#[test]
fn malformed_frames_are_rejected() {
    // Too short to hold the 4-byte length prefix.
    assert!(matches!(Envelope::from_bytes(b"\x00\x00").unwrap_err(), SecurityError::Decryption));
    // Length prefix claims more wrapped-key bytes than are present.
    assert!(matches!(
        Envelope::from_bytes(&[0x00, 0x00, 0x00, 0xFF, 0x01, 0x02]).unwrap_err(),
        SecurityError::Decryption
    ));
    // Not valid base64.
    assert!(matches!(
        Envelope::from_base64("not base64!!").unwrap_err(),
        SecurityError::Decryption
    ));
}
