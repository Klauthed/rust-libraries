#![no_main]

use std::collections::BTreeMap;

use klauthed_core::config::ConfigMap;
use libfuzzer_sys::fuzz_target;
use serde_json::Value;

// Untrusted config trees flow through `expand_dotted` (dotted-key expansion) and
// `merge` (deep merge). Neither tree-shaping pass may panic or blow the stack on
// adversarial structure — e.g. keys with many `.` segments or deep nesting.
fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else { return };
    let Ok(map) = serde_json::from_str::<BTreeMap<String, Value>>(text) else { return };

    let base = ConfigMap::from(map);
    let expanded = base.clone().expand_dotted();

    let mut merged = base;
    merged.merge(expanded);
});
