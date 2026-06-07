//! Public-API integration tests for JWT signing and verification.

use klauthed_core::time::{Duration, FixedClock, SystemClock, Timestamp};
use klauthed_security::{Claims, JwtSigner, JwtVerifier, SecurityError};

/// A clock pinned to "now", so a token minted with a positive TTL is still
/// valid when the verifier checks it against the real wall clock.
fn now_clock() -> FixedClock {
    FixedClock::new(Timestamp::now())
}

#[test]
fn hs256_round_trip() {
    let signer = JwtSigner::hs256(b"shared-secret");
    let verifier = JwtVerifier::hs256(b"shared-secret");

    let claims = Claims::builder("user-1", &now_clock(), Duration::hours(1))
        .issuer("klauthed")
        .audience("api")
        .claim("role", "admin")
        .build();

    let token = signer.encode(&claims).unwrap();
    let decoded =
        verifier.expecting_issuer("klauthed").expecting_audience("api").decode(&token).unwrap();

    assert_eq!(decoded.sub.as_deref(), Some("user-1"));
    assert_eq!(decoded.iss.as_deref(), Some("klauthed"));
    assert_eq!(decoded.custom.get("role").and_then(|v| v.as_str()), Some("admin"));
}

#[test]
fn wrong_secret_is_invalid_token() {
    let token = JwtSigner::hs256(b"key-a")
        .encode(&Claims::builder("u", &now_clock(), Duration::hours(1)).build())
        .unwrap();
    let err = JwtVerifier::hs256(b"key-b").decode(&token).unwrap_err();
    assert!(matches!(err, SecurityError::InvalidToken(_)));
}

#[test]
fn expired_token_is_detected() {
    // exp one hour in the past relative to the signing clock.
    let token = JwtSigner::hs256(b"k")
        .encode(&Claims::builder("u", &now_clock(), Duration::hours(-1)).build())
        .unwrap();
    // Verifier uses the real wall clock, so the past `exp` is expired.
    let err = JwtVerifier::hs256(b"k").decode(&token).unwrap_err();
    assert!(matches!(err, SecurityError::ExpiredToken));
}

#[test]
fn wrong_issuer_is_invalid() {
    let token = JwtSigner::hs256(b"k")
        .encode(&Claims::builder("u", &now_clock(), Duration::hours(1)).issuer("real").build())
        .unwrap();
    let err = JwtVerifier::hs256(b"k").expecting_issuer("expected").decode(&token).unwrap_err();
    assert!(matches!(err, SecurityError::InvalidToken(_)));
}

#[test]
fn malformed_token_is_detected() {
    let err = JwtVerifier::hs256(b"k").decode("not.a.jwt").unwrap_err();
    assert!(matches!(err, SecurityError::MalformedToken(_)));
}

#[test]
fn round_trips_with_system_clock() {
    let signer = JwtSigner::hs256(b"k");
    let token =
        signer.encode(&Claims::builder("sys", &SystemClock, Duration::minutes(5)).build()).unwrap();
    let decoded = JwtVerifier::hs256(b"k").decode(&token).unwrap();
    assert_eq!(decoded.sub.as_deref(), Some("sys"));
    assert!(decoded.exp.is_some() && decoded.iat.is_some());
}

#[test]
fn random_jwt_id_is_set() {
    let claims =
        Claims::builder("u", &now_clock(), Duration::hours(1)).random_jwt_id().unwrap().build();
    assert!(claims.jti.is_some());
}
