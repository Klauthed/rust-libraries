#![no_main]

use klauthed_protocol::oauth2::TokenResponse;
use libfuzzer_sys::fuzz_target;

// Deserializing an untrusted OAuth2 token-endpoint response must not panic —
// only parse or error. Exercises the spec-accurate serde wire types.
fuzz_target!(|data: &[u8]| {
    let _ = serde_json::from_slice::<TokenResponse>(data);
});
