#![no_main]

use klauthed_protocol::oidc::{IdTokenClaims, ProviderMetadata};
use libfuzzer_sys::fuzz_target;

// Deserializing an untrusted OIDC discovery document (`ProviderMetadata`) or ID
// token claim set must not panic — only parse or error.
fuzz_target!(|data: &[u8]| {
    let _ = serde_json::from_slice::<ProviderMetadata>(data);
    let _ = serde_json::from_slice::<IdTokenClaims>(data);
});
