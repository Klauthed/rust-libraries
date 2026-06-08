//! [`PasskeyAuthenticator`] — the relying-party wrapper around [`webauthn_rs`].

use webauthn_rs::prelude::{
    AuthenticationResult, CreationChallengeResponse, Passkey, PasskeyAuthentication,
    PasskeyRegistration, PublicKeyCredential, RegisterPublicKeyCredential,
    RequestChallengeResponse, Url, Uuid, WebauthnError,
};
use webauthn_rs::{Webauthn, WebauthnBuilder};

use crate::error::SecurityError;

/// A configured WebAuthn relying party that drives passkey registration and
/// authentication ceremonies.
///
/// Build one per relying party (your site) from its **RP id** (the registrable
/// domain, e.g. `"example.com"`), the **origin** users authenticate from
/// (e.g. `"https://example.com"`), and a human-readable **name**. The
/// authenticator is cheap to clone-free share behind an `Arc` across requests.
///
/// ```no_run
/// use klauthed_security::passkey::{PasskeyAuthenticator, Uuid};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let rp = PasskeyAuthenticator::new("example.com", "https://example.com", "Example")?;
///
/// // 1. Begin registration: send `challenge` to the browser, persist `state`.
/// let (challenge, state) =
///     rp.start_registration(Uuid::new_v4(), "alice", "Alice Example", &[])?;
/// # let _ = (challenge, state);
/// # Ok(())
/// # }
/// ```
pub struct PasskeyAuthenticator {
    webauthn: Webauthn,
}

impl PasskeyAuthenticator {
    /// Configure a relying party.
    ///
    /// * `rp_id` — the [relying-party id](https://www.w3.org/TR/webauthn-3/#rp-id):
    ///   the effective domain (e.g. `"example.com"`), without scheme or port.
    /// * `rp_origin` — the full origin browsers connect from
    ///   (e.g. `"https://example.com"`); must be `https` (or `http://localhost`).
    /// * `rp_name` — a human-readable name shown by some authenticators.
    ///
    /// # Errors
    ///
    /// Returns [`SecurityError::WebauthnConfig`] if `rp_origin` is not a valid URL
    /// or the RP id / origin pair is rejected.
    pub fn new(rp_id: &str, rp_origin: &str, rp_name: &str) -> Result<Self, SecurityError> {
        let origin = Url::parse(rp_origin)
            .map_err(|e| SecurityError::WebauthnConfig(format!("invalid origin URL: {e}")))?;
        let webauthn = WebauthnBuilder::new(rp_id, &origin)
            .map_err(|e| SecurityError::WebauthnConfig(e.to_string()))?
            .rp_name(rp_name)
            .build()
            .map_err(|e| SecurityError::WebauthnConfig(e.to_string()))?;
        Ok(Self { webauthn })
    }

    /// Begin a registration ceremony for a user.
    ///
    /// Returns the [`CreationChallengeResponse`] to serialize to the browser's
    /// `navigator.credentials.create()`, and the [`PasskeyRegistration`] state to
    /// persist (server-side session) until [`finish_registration`]. Pass the
    /// user's already-registered passkeys as `exclude` so authenticators don't
    /// enroll the same credential twice.
    ///
    /// [`finish_registration`]: Self::finish_registration
    ///
    /// # Errors
    ///
    /// Returns [`SecurityError::Webauthn`] if the ceremony cannot be started.
    pub fn start_registration(
        &self,
        user_id: Uuid,
        user_name: &str,
        user_display_name: &str,
        exclude: &[Passkey],
    ) -> Result<(CreationChallengeResponse, PasskeyRegistration), SecurityError> {
        let exclude_credentials = (!exclude.is_empty())
            .then(|| exclude.iter().map(|passkey| passkey.cred_id().clone()).collect());
        self.webauthn
            .start_passkey_registration(user_id, user_name, user_display_name, exclude_credentials)
            .map_err(map_err)
    }

    /// Complete a registration ceremony, verifying the browser's response against
    /// the persisted state and returning the [`Passkey`] credential to store for
    /// the user.
    ///
    /// # Errors
    ///
    /// Returns [`SecurityError::Webauthn`] if verification fails (challenge
    /// mismatch, bad attestation, origin/RP mismatch, …).
    pub fn finish_registration(
        &self,
        response: &RegisterPublicKeyCredential,
        state: &PasskeyRegistration,
    ) -> Result<Passkey, SecurityError> {
        self.webauthn.finish_passkey_registration(response, state).map_err(map_err)
    }

    /// Begin an authentication ceremony against a user's registered passkeys.
    ///
    /// Returns the [`RequestChallengeResponse`] to serialize to the browser's
    /// `navigator.credentials.get()`, and the [`PasskeyAuthentication`] state to
    /// persist until [`finish_authentication`].
    ///
    /// [`finish_authentication`]: Self::finish_authentication
    ///
    /// # Errors
    ///
    /// Returns [`SecurityError::Webauthn`] if the ceremony cannot be started
    /// (e.g. the user has no registered passkeys).
    pub fn start_authentication(
        &self,
        credentials: &[Passkey],
    ) -> Result<(RequestChallengeResponse, PasskeyAuthentication), SecurityError> {
        self.webauthn.start_passkey_authentication(credentials).map_err(map_err)
    }

    /// Complete an authentication ceremony, verifying the browser's assertion.
    ///
    /// On success returns an [`AuthenticationResult`]; feed it to
    /// [`Passkey::update_credential`] on the stored credential to persist the
    /// updated signature counter (clone-detection).
    ///
    /// # Errors
    ///
    /// Returns [`SecurityError::Webauthn`] if the assertion does not verify.
    pub fn finish_authentication(
        &self,
        response: &PublicKeyCredential,
        state: &PasskeyAuthentication,
    ) -> Result<AuthenticationResult, SecurityError> {
        self.webauthn.finish_passkey_authentication(response, state).map_err(map_err)
    }
}

/// Map a [`WebauthnError`] into the crate's error taxonomy.
fn map_err(error: WebauthnError) -> SecurityError {
    SecurityError::Webauthn(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rp() -> PasskeyAuthenticator {
        PasskeyAuthenticator::new("localhost", "http://localhost:8080", "Test RP")
            .expect("valid RP config")
    }

    #[test]
    fn rejects_invalid_origin() {
        // `PasskeyAuthenticator` isn't `Debug`, so match the Result directly.
        assert!(matches!(
            PasskeyAuthenticator::new("example.com", "not a url", "X"),
            Err(SecurityError::WebauthnConfig(_))
        ));
    }

    #[test]
    fn rejects_origin_rp_mismatch() {
        // RP id must be a registrable suffix of the origin's host.
        assert!(matches!(
            PasskeyAuthenticator::new("other.com", "https://example.com", "X"),
            Err(SecurityError::WebauthnConfig(_))
        ));
    }

    #[test]
    fn start_registration_yields_a_challenge_and_state() {
        let rp = rp();
        let (challenge, state) =
            rp.start_registration(Uuid::new_v4(), "alice", "Alice", &[]).expect("start");
        // The challenge serializes to the JSON the browser API consumes.
        let json = serde_json::to_string(&challenge).expect("serialize challenge");
        assert!(json.contains("challenge"));
        // The ceremony state round-trips through serde for session storage.
        let blob = serde_json::to_string(&state).expect("serialize state");
        let _restored: PasskeyRegistration =
            serde_json::from_str(&blob).expect("deserialize state");
    }

    #[test]
    fn authentication_state_round_trips_through_serde() {
        // An empty credential list is accepted (discoverable-credential flow);
        // the ceremony state must survive serialization for session storage.
        let (_challenge, state) = rp().start_authentication(&[]).expect("start authentication");
        let blob = serde_json::to_string(&state).expect("serialize state");
        let _restored: PasskeyAuthentication =
            serde_json::from_str(&blob).expect("deserialize state");
    }
}
