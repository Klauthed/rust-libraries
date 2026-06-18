#![no_main]

use klauthed_protocol::jwks::{JsonWebKey, JsonWebKeySet};
use libfuzzer_sys::fuzz_target;

// Deserializing an untrusted JWKS / JWK document (e.g. fetched from a remote
// `jwks_uri`) must not panic — only parse or error.
fuzz_target!(|data: &[u8]| {
    let _ = serde_json::from_slice::<JsonWebKeySet>(data);
    let _ = serde_json::from_slice::<JsonWebKey>(data);
});
