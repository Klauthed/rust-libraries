//! Public-API integration tests for `klauthed_security::password`.

use klauthed_security::SecurityError;
use klauthed_security::password::*;

#[test]
fn round_trip_accepts_correct_password() {
    let phc = hash_password("s3cret-pa55").unwrap();
    assert!(phc.starts_with("$argon2id$"));
    assert!(verify_password("s3cret-pa55", &phc).unwrap());
}

#[test]
fn rejects_wrong_password() {
    let phc = hash_password("s3cret-pa55").unwrap();
    assert!(!verify_password("wrong", &phc).unwrap());
}

#[test]
fn salts_are_random_per_hash() {
    let a = hash_password("same").unwrap();
    let b = hash_password("same").unwrap();
    assert_ne!(a, b, "each hash must use a fresh random salt");
    assert!(verify_password("same", &a).unwrap());
    assert!(verify_password("same", &b).unwrap());
}

#[test]
fn malformed_hash_is_an_error() {
    let err = verify_password("x", "not-a-phc-string").unwrap_err();
    assert!(matches!(err, SecurityError::InvalidHash(_)));
}
