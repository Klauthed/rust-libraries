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

#[test]
fn rs256_signer_rejects_invalid_pem() {
    // A non-PEM blob can't be parsed as an RSA private key. (JwtSigner isn't
    // Debug, so match the Result rather than unwrap_err.)
    assert!(matches!(
        JwtSigner::rs256_pem(b"-----BEGIN PRIVATE KEY-----\nnope\n-----END PRIVATE KEY-----"),
        Err(SecurityError::Key(_))
    ));
}

#[test]
fn rs256_verifier_rejects_invalid_pem() {
    assert!(matches!(JwtVerifier::rs256_pem(b"not a public key"), Err(SecurityError::Key(_))));
}

#[test]
fn audience_mismatch_is_invalid() {
    let token = JwtSigner::hs256(b"k")
        .encode(&Claims::builder("u", &now_clock(), Duration::hours(1)).audience("api").build())
        .unwrap();
    let err = JwtVerifier::hs256(b"k").expecting_audience("other").decode(&token).unwrap_err();
    assert!(matches!(err, SecurityError::InvalidToken(_)));
}

#[test]
fn leeway_admits_recently_expired_token() {
    // `exp` is 90s in the past — beyond jsonwebtoken's default 60s leeway.
    let token = JwtSigner::hs256(b"k")
        .encode(&Claims::builder("u", &now_clock(), Duration::seconds(-90)).build())
        .unwrap();

    // Default verifier (60s leeway) rejects it as expired...
    assert!(matches!(
        JwtVerifier::hs256(b"k").decode(&token).unwrap_err(),
        SecurityError::ExpiredToken
    ));

    // ...but a wider leeway window accepts it.
    let decoded = JwtVerifier::hs256(b"k").leeway_seconds(120).decode(&token).unwrap();
    assert_eq!(decoded.sub.as_deref(), Some("u"));
}

// ── ES256 / EdDSA ───────────────────────────────────────────────────────────
//
// Keys are generated at runtime with `ring` (already a dependency) so no private
// key material is committed to the repo. jsonwebtoken's *_der constructors take
// the same formats ring produces: PKCS#8 for private keys, the raw public point
// / 32-byte Ed25519 public key for verification.

use ring::rand::SystemRandom;
use ring::signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair, Ed25519KeyPair, KeyPair};

/// (PKCS#8 private DER, raw 32-byte public key) for a fresh Ed25519 keypair.
fn ed25519_keypair() -> (Vec<u8>, Vec<u8>) {
    let rng = SystemRandom::new();
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
    let kp = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
    (pkcs8.as_ref().to_vec(), kp.public_key().as_ref().to_vec())
}

/// (PKCS#8 private DER, raw public point) for a fresh P-256 keypair.
fn p256_keypair() -> (Vec<u8>, Vec<u8>) {
    let rng = SystemRandom::new();
    let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &rng).unwrap();
    let kp =
        EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, pkcs8.as_ref(), &rng).unwrap();
    (pkcs8.as_ref().to_vec(), kp.public_key().as_ref().to_vec())
}

#[test]
fn eddsa_der_round_trip() {
    let (priv_der, pub_der) = ed25519_keypair();
    let token = JwtSigner::eddsa_der(&priv_der)
        .encode(&Claims::builder("ed-user", &now_clock(), Duration::hours(1)).build())
        .unwrap();
    let decoded = JwtVerifier::eddsa_der(&pub_der).decode(&token).unwrap();
    assert_eq!(decoded.sub.as_deref(), Some("ed-user"));
}

#[test]
fn es256_der_round_trip() {
    let (priv_der, pub_der) = p256_keypair();
    let token = JwtSigner::es256_der(&priv_der)
        .encode(&Claims::builder("ec-user", &now_clock(), Duration::hours(1)).build())
        .unwrap();
    let decoded = JwtVerifier::es256_der(&pub_der).decode(&token).unwrap();
    assert_eq!(decoded.sub.as_deref(), Some("ec-user"));
}

#[test]
fn eddsa_wrong_public_key_is_invalid() {
    let (priv_a, _) = ed25519_keypair();
    let (_, pub_b) = ed25519_keypair();
    let token = JwtSigner::eddsa_der(&priv_a)
        .encode(&Claims::builder("u", &now_clock(), Duration::hours(1)).build())
        .unwrap();
    let err = JwtVerifier::eddsa_der(&pub_b).decode(&token).unwrap_err();
    assert!(matches!(err, SecurityError::InvalidToken(_)));
}

#[test]
fn es256_and_eddsa_pem_reject_invalid_input() {
    assert!(matches!(JwtSigner::es256_pem(b"nope"), Err(SecurityError::Key(_))));
    assert!(matches!(JwtVerifier::es256_pem(b"nope"), Err(SecurityError::Key(_))));
    assert!(matches!(JwtSigner::eddsa_pem(b"nope"), Err(SecurityError::Key(_))));
    assert!(matches!(JwtVerifier::eddsa_pem(b"nope"), Err(SecurityError::Key(_))));
}
