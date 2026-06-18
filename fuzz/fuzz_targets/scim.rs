#![no_main]

use klauthed_protocol::scim::{Group, PatchOp, User};
use libfuzzer_sys::fuzz_target;

// Deserializing an untrusted SCIM provisioning payload — a User/Group resource
// or a PATCH operation set — must not panic, only parse or error.
fuzz_target!(|data: &[u8]| {
    let _ = serde_json::from_slice::<User>(data);
    let _ = serde_json::from_slice::<Group>(data);
    let _ = serde_json::from_slice::<PatchOp>(data);
});
