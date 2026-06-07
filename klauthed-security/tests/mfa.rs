//! Public-API integration tests for TOTP: generate/verify, the ±skew window,
//! provisioning URIs, base32 round-trips, and error mapping.

use klauthed_core::time::{FixedClock, Timestamp};
use klauthed_error::{DomainError, ErrorCategory};
use klauthed_security::SecurityError;
use klauthed_security::mfa::{Totp, TotpSecret};

fn totp() -> Totp {
    Totp::new(TotpSecret::generate(), "klauthed", "alice@example.com").unwrap()
}

#[test]
fn generate_then_verify_succeeds() {
    let t = totp();
    let clock = FixedClock::at_unix_millis(1_700_000_000_000);
    let code = t.generate(&clock);
    assert_eq!(code.len(), 6);
    assert!(t.verify(&code, &clock));
}

#[test]
fn wrong_code_is_rejected() {
    let t = totp();
    let clock = FixedClock::at_unix_millis(1_700_000_000_000);
    let code = t.generate(&clock);
    // Pick a code guaranteed different from the real one.
    let wrong = if code == "000000" { "000001" } else { "000000" };
    assert!(!t.verify(wrong, &clock));
}

#[test]
fn verify_or_err_maps_to_unauthorized() {
    let t = totp();
    let clock = FixedClock::at_unix_millis(1_700_000_000_000);
    let code = t.generate(&clock);
    let wrong = if code == "999999" { "999998" } else { "999999" };
    let err = t.verify_or_err(wrong, &clock).unwrap_err();
    assert!(matches!(err, SecurityError::InvalidMfaCode));
    assert_eq!(err.category(), ErrorCategory::Unauthorized);
    assert_eq!(err.code().as_str(), "security.invalid_mfa_code");
}

#[test]
fn code_accepted_within_skew_window() {
    let t = totp();
    let base = FixedClock::at_unix_millis(1_700_000_000_000);
    let code = t.generate(&base);

    // One step (30s) earlier and later are still accepted (skew = 1).
    let earlier = FixedClock::new(Timestamp::from_unix_millis(1_700_000_000_000 - 30_000));
    let later = FixedClock::new(Timestamp::from_unix_millis(1_700_000_000_000 + 30_000));
    assert!(t.verify(&code, &earlier));
    assert!(t.verify(&code, &later));
}

#[test]
fn code_rejected_outside_skew_window() {
    let t = totp();
    let base = FixedClock::at_unix_millis(1_700_000_000_000);
    let code = t.generate(&base);
    // Two steps away (60s) is outside the ±1 window.
    let far = FixedClock::new(Timestamp::from_unix_millis(1_700_000_000_000 + 60_000));
    assert!(!t.verify(&code, &far));
}

#[test]
fn provisioning_uri_is_otpauth_with_issuer_and_account() {
    let t = totp();
    let uri = t.provisioning_uri();
    assert!(uri.starts_with("otpauth://totp/"));
    assert!(uri.contains("klauthed"));
    assert!(uri.contains("secret="));
}

#[test]
fn restore_from_base32_round_trips() {
    let secret = TotpSecret::generate();
    let b32 = secret.as_base32().to_owned();
    let restored = Totp::new(TotpSecret::from_base32(&b32), "klauthed", "bob").unwrap();
    assert_eq!(restored.secret_base32(), b32);
}

#[test]
fn invalid_base32_secret_is_mfa_config_error() {
    // '1', '8', '0' are not in the RFC 4648 base32 alphabet, but the secret
    // must also be long enough; use clearly invalid chars.
    let err = Totp::new(TotpSecret::from_base32("not valid base32!!!"), "k", "a").unwrap_err();
    assert!(matches!(err, SecurityError::MfaConfig(_)));
    assert_eq!(err.category(), ErrorCategory::Internal);
}

#[test]
fn secret_debug_does_not_leak() {
    let s = TotpSecret::from_base32("JBSWY3DPEHPK3PXP");
    assert_eq!(format!("{s:?}"), "TotpSecret(***)");
}
