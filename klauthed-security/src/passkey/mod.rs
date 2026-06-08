//! WebAuthn passkeys — registration and authentication ceremonies.
//!
//! A thin, klauthed-flavoured wrapper over the vetted [`webauthn_rs`] crate
//! (the de-facto Rust relying-party implementation). Only available with the
//! `webauthn` feature, which pulls in `webauthn-rs` (MPL-2.0) and links OpenSSL.
//!
//! [`PasskeyAuthenticator`] drives the two WebAuthn ceremonies, each a pair of
//! round-trips between server and browser:
//!
//! * **Registration** — [`start_registration`] hands the browser a challenge and
//!   gives you a [`PasskeyRegistration`] state to persist;
//!   [`finish_registration`] verifies the browser's response and yields a
//!   [`Passkey`] to store for the user.
//! * **Authentication** — [`start_authentication`] challenges against the user's
//!   stored passkeys and gives you a [`PasskeyAuthentication`] state;
//!   [`finish_authentication`] verifies the assertion.
//!
//! The challenge/state/credential types all implement `serde`, so the challenge
//! is sent to the browser as JSON, the ceremony state is parked in the user's
//! session between round-trips, and the [`Passkey`] is persisted as JSON on the
//! user record. (Persisting ceremony state relies on the
//! `danger-allow-state-serialisation` feature of `webauthn-rs`, enabled here.)
//!
//! [`start_registration`]: PasskeyAuthenticator::start_registration
//! [`finish_registration`]: PasskeyAuthenticator::finish_registration
//! [`start_authentication`]: PasskeyAuthenticator::start_authentication
//! [`finish_authentication`]: PasskeyAuthenticator::finish_authentication

pub mod authenticator;

pub use authenticator::PasskeyAuthenticator;

// Re-export the `webauthn-rs` types that appear in this module's public API, so
// callers don't need a direct dependency on `webauthn-rs`.
pub use webauthn_rs::prelude::{
    AuthenticationResult, CreationChallengeResponse, Passkey, PasskeyAuthentication,
    PasskeyRegistration, PublicKeyCredential, RegisterPublicKeyCredential,
    RequestChallengeResponse, Uuid,
};
