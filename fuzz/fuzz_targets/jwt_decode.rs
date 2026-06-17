#![no_main]

use klauthed_security::JwtVerifier;
use libfuzzer_sys::fuzz_target;

// Verifying an attacker-controlled JWT must never panic: decoding arbitrary
// token text may only ever return `Ok(claims)` or `Err(SecurityError)`.
fuzz_target!(|data: &[u8]| {
    if let Ok(token) = std::str::from_utf8(data) {
        let verifier = JwtVerifier::hs256(b"fuzzing-shared-secret");
        let _ = verifier.decode(token);
    }
});
