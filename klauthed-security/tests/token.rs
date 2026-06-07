//! Public-API integration tests for `klauthed_security::token`.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use klauthed_security::compare::constant_time_eq;
use klauthed_security::token::*;

#[test]
fn random_bytes_have_requested_length() {
    assert_eq!(random_bytes(0).unwrap().len(), 0);
    assert_eq!(random_bytes(48).unwrap().len(), 48);
}

#[test]
fn tokens_are_url_safe_and_unique() {
    let a = random_token(32).unwrap();
    let b = random_token(32).unwrap();
    assert_ne!(a, b);
    for t in [&a, &b] {
        assert!(t.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'));
    }
    assert!(!constant_time_eq(a.as_bytes(), b.as_bytes()));
}

#[test]
fn decodes_back_to_requested_entropy() {
    let t = random_token(16).unwrap();
    let decoded = URL_SAFE_NO_PAD.decode(t).unwrap();
    assert_eq!(decoded.len(), 16);
}
