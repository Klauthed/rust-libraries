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

pub mod secret;
pub mod totp;

pub use secret::TotpSecret;
pub use totp::Totp;
