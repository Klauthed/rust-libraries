//! Public-API integration tests for `klauthed_security::kdf`.

use klauthed_security::SecurityError;
use klauthed_security::kdf::*;

#[test]
fn deterministic_for_same_inputs() {
    let a = derive_key(b"ikm", b"salt", b"info", 32).unwrap();
    let b = derive_key(b"ikm", b"salt", b"info", 32).unwrap();
    assert_eq!(a, b);
    assert_eq!(a.len(), 32);
}

#[test]
fn different_info_diverges() {
    let a = derive_key(b"ikm", b"salt", b"info-a", 32).unwrap();
    let b = derive_key(b"ikm", b"salt", b"info-b", 32).unwrap();
    assert_ne!(a, b);
}

#[test]
fn different_salt_diverges() {
    let a = derive_key(b"ikm", b"salt-a", b"info", 32).unwrap();
    let b = derive_key(b"ikm", b"salt-b", b"info", 32).unwrap();
    assert_ne!(a, b);
}

#[test]
fn different_ikm_diverges() {
    let a = derive_key(b"ikm-a", b"salt", b"info", 32).unwrap();
    let b = derive_key(b"ikm-b", b"salt", b"info", 32).unwrap();
    assert_ne!(a, b);
}

#[test]
fn respects_requested_length() {
    assert_eq!(derive_key(b"ikm", b"s", b"i", 16).unwrap().len(), 16);
    assert_eq!(derive_key(b"ikm", b"s", b"i", 64).unwrap().len(), 64);
}

#[test]
fn too_long_output_errors() {
    let err = derive_key(b"ikm", b"s", b"i", 255 * 32 + 1).unwrap_err();
    assert!(matches!(err, SecurityError::KeyDerivation));
}

#[test]
fn derive_key_32_matches_derive_key() {
    let a = derive_key_32(b"ikm", b"salt", b"info").unwrap();
    let b = derive_key(b"ikm", b"salt", b"info", 32).unwrap();
    assert_eq!(&a[..], &b[..]);
}

// RFC 5869 Test Case 1 (HKDF-SHA256) known-answer.
#[test]
fn rfc5869_test_case_1() {
    let ikm = [0x0bu8; 22];
    let salt: Vec<u8> = (0x00u8..=0x0c).collect();
    let info: Vec<u8> = (0xf0u8..=0xf9).collect();
    let okm = derive_key(&ikm, &salt, &info, 42).unwrap();
    let expected = hex::decode(
        "3cb25f25faacd57a90434f64d0362f2a\
         2d2d0a90cf1a5a4c5db02d56ecc4c5bf\
         34007208d5b887185865",
    )
    .unwrap();
    assert_eq!(okm, expected);
}
