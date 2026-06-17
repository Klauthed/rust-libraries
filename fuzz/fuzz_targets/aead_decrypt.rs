#![no_main]

use klauthed_security::{decrypt, decrypt_from_base64, EncryptionKey};
use libfuzzer_sys::fuzz_target;

// Opening attacker-controlled ciphertext must reject cleanly rather than panic,
// regardless of how short or malformed the input (nonce slicing, base64, tag).
fuzz_target!(|data: &[u8]| {
    let key = EncryptionKey::from_bytes([7u8; 32]);
    let _ = decrypt(&key, data, b"");
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = decrypt_from_base64(&key, text, b"");
    }
});
