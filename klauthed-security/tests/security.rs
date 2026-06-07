//! Public-API integration tests for the security primitives.

use klauthed_core::time::{Duration, SystemClock};
use klauthed_security::{Claims, JwtSigner, JwtVerifier, hash_password, verify_password};

#[test]
fn password_hash_round_trips() {
    let hash = hash_password("correct horse battery staple").unwrap();
    assert!(verify_password("correct horse battery staple", &hash).unwrap());
    assert!(!verify_password("wrong password", &hash).unwrap());
}

#[test]
fn jwt_sign_and_verify_round_trips() {
    let signer = JwtSigner::hs256(b"shared-signing-secret");
    let verifier = JwtVerifier::hs256(b"shared-signing-secret");

    let claims = Claims::builder("user-1", &SystemClock, Duration::hours(1))
        .issuer("klauthed")
        .audience("klauthed-api")
        .build();
    let token = signer.encode(&claims).unwrap();

    let decoded = verifier
        .expecting_issuer("klauthed")
        .expecting_audience("klauthed-api")
        .decode(&token)
        .unwrap();
    assert_eq!(decoded.sub.as_deref(), Some("user-1"));
}

#[test]
fn jwt_with_wrong_secret_is_rejected() {
    let token = JwtSigner::hs256(b"the-real-secret")
        .encode(&Claims::builder("u", &SystemClock, Duration::hours(1)).build())
        .unwrap();
    let result = JwtVerifier::hs256(b"a-different-secret").decode(&token);
    assert!(result.is_err());
}
