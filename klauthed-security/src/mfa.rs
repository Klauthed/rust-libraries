//! Time-based one-time passwords (TOTP, [RFC 6238]).
//!
//! Wraps the vetted [`totp-rs`](totp_rs) crate behind a klauthed-flavoured API:
//!
//! * [`TotpSecret`] — a base32 shared secret, freshly generated from a CSPRNG or
//!   restored from a stored value.
//! * [`Totp`] — a configured authenticator (issuer, account, 6 digits, 30s step,
//!   ±1 step window by default) that can build the `otpauth://` provisioning URI
//!   and verify a submitted code.
//!
//! "Current time" is taken from a [`klauthed_core::time::Clock`], so code
//! generation/verification is testable with
//! [`FixedClock`](klauthed_core::time::FixedClock).
//!
//! [RFC 6238]: https://datatracker.ietf.org/doc/html/rfc6238
//!
//! ```
//! use klauthed_security::mfa::{Totp, TotpSecret};
//! use klauthed_core::time::{FixedClock, Timestamp};
//!
//! let secret = TotpSecret::generate();
//! let totp = Totp::new(secret, "klauthed", "alice@example.com").unwrap();
//!
//! // A provisioning URI to render as a QR code in an authenticator app.
//! assert!(totp.provisioning_uri().starts_with("otpauth://totp/"));
//!
//! // Generate the current code and verify it against the same clock.
//! let clock = FixedClock::at_unix_millis(1_700_000_000_000);
//! let code = totp.generate(&clock);
//! assert!(totp.verify(&code, &clock));
//! assert!(!totp.verify("000000", &clock) || code == "000000");
//! ```

use totp_rs::{Algorithm, Secret, TOTP};

use klauthed_core::time::Clock;

use crate::error::SecurityError;

/// Default number of digits in a generated code (RFC 6238 §1.2 example uses 6).
const DEFAULT_DIGITS: usize = 6;
/// Default step length in seconds (RFC 6238 §5.2 recommendation).
const DEFAULT_STEP_SECS: u64 = 30;
/// Default skew: ±1 step accepted, to tolerate clock drift / latency.
const DEFAULT_SKEW: u8 = 1;

/// A TOTP shared secret, held as its base32 (RFC 4648, unpadded) string form —
/// the representation users see and authenticator apps consume.
#[derive(Clone, PartialEq, Eq)]
pub struct TotpSecret(String);

impl TotpSecret {
    /// Generate a fresh 160-bit secret from a CSPRNG (RFC 4226's recommended
    /// length), stored as base32.
    #[must_use]
    pub fn generate() -> Self {
        // `generate_secret` returns a `Secret::Raw`; re-encode to base32 text.
        Self(Secret::generate_secret().to_encoded().to_string())
    }

    /// Restore a secret from its stored base32 string. The value is not decoded
    /// here; an invalid base32 string surfaces later as
    /// [`SecurityError::MfaConfig`] when building a [`Totp`].
    pub fn from_base32(secret: impl Into<String>) -> Self {
        Self(secret.into())
    }

    /// The base32 secret string (e.g. to persist, or show for manual entry).
    #[must_use]
    pub fn as_base32(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for TotpSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print the secret material.
        f.write_str("TotpSecret(***)")
    }
}

/// A configured TOTP authenticator for one account.
pub struct Totp {
    inner: TOTP,
}

impl Totp {
    /// Build an authenticator with the default profile: SHA-1, 6 digits, a 30s
    /// step and a ±1 step verification window.
    ///
    /// `issuer` is the service name (e.g. `"klauthed"`) and `account` identifies
    /// the user within it (e.g. an email). Neither may contain a colon.
    ///
    /// # Errors
    /// [`SecurityError::MfaConfig`] if the secret is not valid base32, is shorter
    /// than RFC 4226's 128-bit minimum, or `issuer`/`account` contain a colon.
    pub fn new(
        secret: TotpSecret,
        issuer: impl Into<String>,
        account: impl Into<String>,
    ) -> Result<Self, SecurityError> {
        Self::with_step(secret, issuer, account, DEFAULT_STEP_SECS, DEFAULT_SKEW)
    }

    /// Like [`new`](Totp::new) but with an explicit `step` (seconds per code) and
    /// `skew` (number of steps accepted on each side of "now").
    ///
    /// # Errors
    /// As [`new`](Totp::new).
    pub fn with_step(
        secret: TotpSecret,
        issuer: impl Into<String>,
        account: impl Into<String>,
        step: u64,
        skew: u8,
    ) -> Result<Self, SecurityError> {
        let bytes = Secret::Encoded(secret.0)
            .to_bytes()
            .map_err(|e| SecurityError::MfaConfig(format!("invalid base32 secret: {e:?}")))?;

        let inner = TOTP::new(
            Algorithm::SHA1,
            DEFAULT_DIGITS,
            skew,
            step,
            bytes,
            Some(issuer.into()),
            account.into(),
        )
        .map_err(|e| SecurityError::MfaConfig(e.to_string()))?;

        Ok(Self { inner })
    }

    /// The `otpauth://totp/...` provisioning URI to enroll this account in an
    /// authenticator app (commonly rendered as a QR code).
    #[must_use]
    pub fn provisioning_uri(&self) -> String {
        self.inner.get_url()
    }

    /// The base32 secret (e.g. to display alongside the QR code for manual entry).
    #[must_use]
    pub fn secret_base32(&self) -> String {
        self.inner.get_secret_base32()
    }

    /// The current code for the instant reported by `clock`.
    #[must_use]
    pub fn generate<C: Clock + ?Sized>(&self, clock: &C) -> String {
        self.inner.generate(self.unix_secs(clock))
    }

    /// Whether `code` is valid for the instant reported by `clock`, accepting the
    /// configured ±`skew` step window.
    #[must_use]
    pub fn verify<C: Clock + ?Sized>(&self, code: &str, clock: &C) -> bool {
        self.inner.check(code, self.unix_secs(clock))
    }

    /// Verify and turn a mismatch into [`SecurityError::InvalidMfaCode`].
    ///
    /// # Errors
    /// [`SecurityError::InvalidMfaCode`] when `code` is not valid for `clock`.
    pub fn verify_or_err<C: Clock + ?Sized>(
        &self,
        code: &str,
        clock: &C,
    ) -> Result<(), SecurityError> {
        if self.verify(code, clock) { Ok(()) } else { Err(SecurityError::InvalidMfaCode) }
    }

    /// Whole seconds since the Unix epoch for `clock`'s current instant.
    fn unix_secs<C: Clock + ?Sized>(&self, clock: &C) -> u64 {
        clock.now().unix_seconds().max(0).unsigned_abs()
    }
}

impl std::fmt::Debug for Totp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Totp")
            .field("digits", &self.inner.digits)
            .field("step", &self.inner.step)
            .field("skew", &self.inner.skew)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use klauthed_core::time::{FixedClock, Timestamp};
    use klauthed_error::{DomainError, ErrorCategory};

    fn totp() -> Totp {
        Totp::new(TotpSecret::generate(), "klauthed", "alice@example.com").unwrap()
    }

    #[test]
    fn generate_then_verify_succeeds() {
        let t = totp();
        let clock = FixedClock::at_unix_millis(1_700_000_000_000);
        let code = t.generate(&clock);
        assert_eq!(code.len(), DEFAULT_DIGITS);
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
}
