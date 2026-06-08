//! `klauthed-security`: JWTs, password hashing, AEAD (symmetric, envelope,
//! sealed-box), and TOTP.

use klauthed_core::time::{Duration, FixedClock, SystemClock};
use klauthed_security::aead::{self, asymmetric};
use klauthed_security::mfa::{Totp, TotpSecret};
use klauthed_security::{Claims, JwtSigner, JwtVerifier, hash_password, verify_password};

pub fn run() {
    // ── JWT (HS256) ──
    let signer = JwtSigner::hs256(b"shared-signing-secret");
    let claims = Claims::builder("user-1", &SystemClock, Duration::hours(1))
        .issuer("klauthed-demo")
        .claim("role", "admin")
        .build();
    let token = signer.encode(&claims).unwrap();
    let decoded = JwtVerifier::hs256(b"shared-signing-secret")
        .expecting_issuer("klauthed-demo")
        .decode(&token)
        .unwrap();
    println!(
        "  jwt: signed + verified sub={:?} role={:?}",
        decoded.sub.as_deref(),
        decoded.custom.get("role").and_then(|v| v.as_str())
    );
    assert_eq!(decoded.sub.as_deref(), Some("user-1"));

    // ── Password hashing (Argon2) ──
    let hash = hash_password("hunter2").unwrap();
    println!(
        "  password: hash verifies={}, wrong-rejected={}",
        verify_password("hunter2", &hash).unwrap(),
        !verify_password("nope", &hash).unwrap()
    );
    assert!(verify_password("hunter2", &hash).unwrap());
    assert!(!verify_password("nope", &hash).unwrap());

    // ── AEAD: symmetric ──
    let key = aead::EncryptionKey::generate().unwrap();
    let ct = aead::encrypt(&key, b"top secret", b"ctx:1").unwrap();
    assert_eq!(aead::decrypt(&key, &ct, b"ctx:1").unwrap(), b"top secret");
    assert!(aead::decrypt(&key, &ct, b"ctx:2").is_err()); // wrong AAD rejected
    println!("  aead: round-trip ok; wrong-AAD rejected");

    // ── AEAD: envelope encryption + root-key rotation ──
    let root = aead::EncryptionKey::generate().unwrap();
    let env = aead::seal(&root, b"card number", b"rec:1").unwrap();
    let new_root = aead::EncryptionKey::generate().unwrap();
    let rotated = env.rewrap(&root, &new_root).unwrap();
    assert_eq!(rotated.open(&new_root, b"rec:1").unwrap(), b"card number");
    assert!(env.open(&new_root, b"rec:1").is_err()); // old envelope, new root: no
    println!("  envelope: sealed, rewrapped to a new root key, opened");

    // ── AEAD: sealed-box (public-key) ──
    let recipient = asymmetric::KeyPair::generate().unwrap();
    let sealed = asymmetric::seal_to(recipient.public(), b"for your eyes only", b"").unwrap();
    assert_eq!(asymmetric::open(recipient.secret(), &sealed, b"").unwrap(), b"for your eyes only");
    println!("  sealed-box: encrypted to a public key, opened with the secret key");

    // ── MFA (TOTP) ──
    let totp = Totp::new(TotpSecret::generate(), "klauthed", "alice@example.com").unwrap();
    let clock = FixedClock::at_unix_millis(1_700_000_000_000);
    let code = totp.generate(&clock);
    assert!(totp.verify(&code, &clock));
    println!("  totp: generated {code} and verified it against the same clock");
}
