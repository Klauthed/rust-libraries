//! Public-API integration tests for the AEAD module: round-trips, nonce
//! freshness, and the tamper/wrong-key/wrong-AAD rejection paths.

use klauthed_security::SecurityError;
use klauthed_security::aead::{
    EncryptionKey, KEY_LEN, decrypt, decrypt_from_base64, encrypt, encrypt_to_base64,
};
use ring::aead::NONCE_LEN;

#[test]
fn round_trip_recovers_plaintext() {
    let key = EncryptionKey::generate().unwrap();
    let pt = b"the quick brown fox";
    let aad = b"ctx:1";
    let ct = encrypt(&key, pt, aad).unwrap();
    // Output carries the prepended nonce and the 16-byte GCM tag.
    assert_eq!(ct.len(), NONCE_LEN + pt.len() + 16);
    assert_eq!(decrypt(&key, &ct, aad).unwrap(), pt);
}

#[test]
fn empty_plaintext_round_trips() {
    let key = EncryptionKey::generate().unwrap();
    let ct = encrypt(&key, b"", b"").unwrap();
    assert_eq!(decrypt(&key, &ct, b"").unwrap(), b"");
}

#[test]
fn nonce_is_random_per_encryption() {
    let key = EncryptionKey::generate().unwrap();
    let a = encrypt(&key, b"same plaintext", b"aad").unwrap();
    let b = encrypt(&key, b"same plaintext", b"aad").unwrap();
    assert_ne!(a, b, "fresh nonce must make ciphertexts differ");
    assert_eq!(decrypt(&key, &a, b"aad").unwrap(), b"same plaintext");
    assert_eq!(decrypt(&key, &b, b"aad").unwrap(), b"same plaintext");
}

#[test]
fn wrong_key_fails() {
    let key = EncryptionKey::generate().unwrap();
    let other = EncryptionKey::generate().unwrap();
    let ct = encrypt(&key, b"secret", b"aad").unwrap();
    let err = decrypt(&other, &ct, b"aad").unwrap_err();
    assert!(matches!(err, SecurityError::Decryption));
}

#[test]
fn tampered_ciphertext_fails() {
    let key = EncryptionKey::generate().unwrap();
    let mut ct = encrypt(&key, b"secret", b"aad").unwrap();
    // Flip a bit in the ciphertext body (past the nonce).
    let i = NONCE_LEN + 1;
    ct[i] ^= 0x01;
    assert!(matches!(decrypt(&key, &ct, b"aad").unwrap_err(), SecurityError::Decryption));
}

#[test]
fn tampered_tag_fails() {
    let key = EncryptionKey::generate().unwrap();
    let mut ct = encrypt(&key, b"secret", b"aad").unwrap();
    let last = ct.len() - 1;
    ct[last] ^= 0x80;
    assert!(matches!(decrypt(&key, &ct, b"aad").unwrap_err(), SecurityError::Decryption));
}

#[test]
fn wrong_aad_fails() {
    let key = EncryptionKey::generate().unwrap();
    let ct = encrypt(&key, b"secret", b"record:1").unwrap();
    assert!(matches!(decrypt(&key, &ct, b"record:2").unwrap_err(), SecurityError::Decryption));
}

#[test]
fn too_short_input_fails() {
    let key = EncryptionKey::generate().unwrap();
    assert!(matches!(
        decrypt(&key, &[0u8; NONCE_LEN], b"").unwrap_err(),
        SecurityError::Decryption
    ));
    assert!(matches!(decrypt(&key, b"short", b"").unwrap_err(), SecurityError::Decryption));
}

#[test]
fn base64_round_trip() {
    let key = EncryptionKey::generate().unwrap();
    let s = encrypt_to_base64(&key, b"hello", b"v1").unwrap();
    assert_eq!(decrypt_from_base64(&key, &s, b"v1").unwrap(), b"hello");
    assert!(decrypt_from_base64(&key, "not valid base64!!", b"v1").is_err());
}

#[test]
fn from_bytes_is_deterministic_key() {
    let key = EncryptionKey::from_bytes([42u8; KEY_LEN]);
    let ct = encrypt(&key, b"data", b"").unwrap();
    // A separate key built from identical bytes can decrypt.
    let same = EncryptionKey::from_bytes([42u8; KEY_LEN]);
    assert_eq!(decrypt(&same, &ct, b"").unwrap(), b"data");
}

#[test]
fn debug_does_not_leak_key() {
    let key = EncryptionKey::from_bytes([0xABu8; KEY_LEN]);
    let s = format!("{key:?}");
    assert!(!s.contains("AB") && !s.contains("171"));
}
