//! The [`Totp`] authenticator: provisioning URIs, code generation, and
//! clock-driven verification.

use totp_rs::{Algorithm, Secret, TOTP};

use klauthed_core::time::Clock;

use super::TotpSecret;
use crate::error::SecurityError;

/// Default number of digits in a generated code (RFC 6238 §1.2 example uses 6).
const DEFAULT_DIGITS: usize = 6;
/// Default step length in seconds (RFC 6238 §5.2 recommendation).
const DEFAULT_STEP_SECS: u64 = 30;
/// Default skew: ±1 step accepted, to tolerate clock drift / latency.
const DEFAULT_SKEW: u8 = 1;

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
        let bytes = Secret::Encoded(secret.as_base32().to_owned())
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
