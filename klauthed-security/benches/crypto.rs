//! Micro-benchmarks for the crypto hot paths an auth service runs per request:
//! HS256 JWT signing/verification and AES-256-GCM AEAD. Run with
//! `cargo bench -p klauthed-security`.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use klauthed_core::time::{Duration, FixedClock};
use klauthed_security::{Claims, EncryptionKey, JwtSigner, JwtVerifier, decrypt, encrypt};

const SECRET: &[u8] = b"benchmark-shared-secret-please-rotate";

fn sample_claims() -> Claims {
    let clock = FixedClock::at_unix_millis(1_700_000_000_000);
    Claims::builder("user-1234", &clock, Duration::hours(1)).issuer("klauthed").build()
}

fn bench_jwt(c: &mut Criterion) {
    let signer = JwtSigner::hs256(SECRET);
    let verifier = JwtVerifier::hs256(SECRET);
    let claims = sample_claims();

    c.bench_function("jwt_hs256_sign", |b| {
        b.iter(|| black_box(signer.encode(black_box(&claims)).unwrap()));
    });

    let token = signer.encode(&claims).unwrap();
    c.bench_function("jwt_hs256_verify", |b| {
        b.iter(|| black_box(verifier.decode(black_box(&token)).unwrap()));
    });
}

fn bench_aead(c: &mut Criterion) {
    let key = EncryptionKey::from_bytes([42u8; 32]);
    let plaintext = b"a representative secret payload, around fifty bytes.";

    c.bench_function("aead_encrypt", |b| {
        b.iter(|| black_box(encrypt(&key, black_box(plaintext), b"").unwrap()));
    });

    let ciphertext = encrypt(&key, plaintext, b"").unwrap();
    c.bench_function("aead_decrypt", |b| {
        b.iter(|| black_box(decrypt(&key, black_box(&ciphertext), b"").unwrap()));
    });
}

criterion_group!(benches, bench_jwt, bench_aead);
criterion_main!(benches);
