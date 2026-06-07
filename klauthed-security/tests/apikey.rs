//! Public-API integration tests for `klauthed_security::apikey`.

use klauthed_security::apikey::*;

#[test]
fn generate_then_verify_succeeds() {
    let (key, hash) = generate_api_key("sk").unwrap();
    assert!(key.starts_with("sk_"));
    // 32 bytes base64url-unpadded == 43 chars, plus "sk_".
    assert_eq!(key.len(), "sk_".len() + 43);
    // SHA-256 hex is 64 chars.
    assert_eq!(hash.len(), 64);
    assert!(verify_api_key(&key, &hash));
}

#[test]
fn wrong_key_fails() {
    let (_key, hash) = generate_api_key("pk").unwrap();
    assert!(!verify_api_key("pk_not-the-real-key", &hash));
}

#[test]
fn tampered_hash_fails() {
    let (key, mut hash) = generate_api_key("sk").unwrap();
    // Flip the first hex character to something different.
    let first = hash.remove(0);
    let replacement = if first == 'a' { 'b' } else { 'a' };
    hash.insert(0, replacement);
    assert!(!verify_api_key(&key, &hash));
}

#[test]
fn keys_are_unique() {
    let (a, ha) = generate_api_key("sk").unwrap();
    let (b, hb) = generate_api_key("sk").unwrap();
    assert_ne!(a, b);
    assert_ne!(ha, hb);
}

#[test]
fn prefix_is_preserved_and_bound() {
    let (key, hash) = generate_api_key("pk_live").unwrap();
    assert!(key.starts_with("pk_live_"));
    // The same secret tail under a different prefix would hash differently;
    // here we just confirm the full key verifies.
    assert!(verify_api_key(&key, &hash));
}

#[test]
fn empty_or_malformed_stored_hash_fails() {
    let (key, _hash) = generate_api_key("sk").unwrap();
    assert!(!verify_api_key(&key, ""));
    assert!(!verify_api_key(&key, "deadbeef"));
}
