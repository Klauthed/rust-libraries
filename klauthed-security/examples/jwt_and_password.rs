//! Hash and verify a password, then mint and verify a JWT.
//!
//! Run with: `cargo run -p klauthed-security --example jwt_and_password`

use klauthed_core::time::{Duration, SystemClock};
use klauthed_security::{Claims, JwtSigner, JwtVerifier, hash_password, verify_password};

fn main() {
    // ── Password hashing (Argon2id) ──────────────────────────────────────────
    let hash = hash_password("s3cr3t-password").expect("hashing failed");
    println!("stored PHC hash: {hash}");
    println!("verifies:        {}", verify_password("s3cr3t-password", &hash).unwrap());
    println!("wrong rejected:  {}", !verify_password("nope", &hash).unwrap());

    // ── JWT sign + verify (HS256) ─────────────────────────────────────────────
    let signer = JwtSigner::hs256(b"shared-signing-secret");
    let claims = Claims::builder("alice", &SystemClock, Duration::minutes(15))
        .issuer("klauthed")
        .audience("klauthed-api")
        .build();
    let token = signer.encode(&claims).expect("signing failed");
    println!("\ntoken: {token}");

    let verifier = JwtVerifier::hs256(b"shared-signing-secret");
    let decoded = verifier
        .expecting_issuer("klauthed")
        .expecting_audience("klauthed-api")
        .decode(&token)
        .expect("verification failed");
    println!("subject: {:?}", decoded.sub);
}
