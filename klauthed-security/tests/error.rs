//! Public-API contract test for [`SecurityError`]: every variant's stable
//! `security.<code>`, its `DomainError` category, and a non-empty `Display`.
//!
//! These codes/categories are part of the wire contract (they shape HTTP
//! responses), so this guards against accidental renames or recategorization.

use klauthed_error::{DomainError, ErrorCategory};
use klauthed_security::SecurityError;

/// (variant, expected `code().as_str()`, expected category, a `Display` substring)
fn cases() -> Vec<(SecurityError, &'static str, ErrorCategory, &'static str)> {
    use ErrorCategory as C;
    use SecurityError::*;
    vec![
        (Hash("x".into()), "security.hash", C::Internal, "password hashing failed"),
        (InvalidHash("x".into()), "security.invalid_hash", C::BadRequest, "invalid password hash"),
        (Key("x".into()), "security.key", C::Internal, "invalid or unloadable key"),
        (Encode("x".into()), "security.encode", C::Internal, "token encoding failed"),
        (MalformedToken("x".into()), "security.malformed_token", C::BadRequest, "malformed token"),
        (InvalidToken("x".into()), "security.invalid_token", C::Unauthorized, "invalid token"),
        (ExpiredToken, "security.expired_token", C::Unauthorized, "token has expired"),
        (
            TokenTtlOverflow("refresh token".into()),
            "security.token_ttl_overflow",
            C::Internal,
            "ttl overflowed",
        ),
        (Rng, "security.rng", C::Internal, "random number generator failed"),
        (SessionNotFound, "security.session_not_found", C::NotFound, "session not found"),
        (SessionExpired, "security.session_expired", C::Unauthorized, "session has expired"),
        (Forbidden, "security.forbidden", C::Forbidden, "not authorized"),
        (MfaConfig("x".into()), "security.mfa_config", C::Internal, "invalid MFA configuration"),
        (InvalidMfaCode, "security.invalid_mfa_code", C::Unauthorized, "invalid MFA code"),
        (Encryption, "security.encryption", C::Internal, "authenticated encryption failed"),
        (Decryption, "security.decryption", C::BadRequest, "authenticated decryption failed"),
        (KeyDerivation, "security.key_derivation", C::Internal, "HKDF key derivation failed"),
    ]
}

#[test]
fn every_variant_has_stable_code_category_and_display() {
    for (err, code, category, display_substr) in cases() {
        assert_eq!(err.code().as_str(), code, "code for {err:?}");
        assert_eq!(err.category(), category, "category for {err:?}");
        let shown = err.to_string();
        assert!(
            shown.contains(display_substr),
            "Display for {err:?} = {shown:?} should contain {display_substr:?}",
        );
        assert!(!shown.is_empty());
    }
}

#[test]
fn all_codes_share_the_security_prefix_and_are_unique() {
    let codes: Vec<&str> = cases().iter().map(|(_, code, _, _)| *code).collect();
    assert!(codes.iter().all(|c| c.starts_with("security.")), "every code is namespaced");

    let mut deduped = codes.clone();
    deduped.sort_unstable();
    deduped.dedup();
    assert_eq!(deduped.len(), codes.len(), "codes must be unique across variants");
}
