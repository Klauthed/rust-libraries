//! End-to-end passkey ceremony test, driven by an in-process software
//! authenticator (`webauthn-authenticator-rs`'s `SoftPasskey`) so the full
//! registration → store → authentication flow runs without a browser.
//!
//! Doubles as a worked example of wiring [`PasskeyAuthenticator`] to a
//! [`PasskeyStore`]. Only built with the `webauthn` feature.
#![cfg(feature = "webauthn")]

use klauthed_security::passkey::{
    InMemoryPasskeyStore, PasskeyAuthenticator, PasskeyStore, Url, Uuid,
};
use webauthn_authenticator_rs::WebauthnAuthenticator;
use webauthn_authenticator_rs::softpasskey::SoftPasskey;

const RP_ID: &str = "localhost";
const ORIGIN: &str = "http://localhost:8080";

#[tokio::test]
async fn register_store_then_authenticate() {
    let rp = PasskeyAuthenticator::new(RP_ID, ORIGIN, "Test RP").expect("configure RP");
    let store = InMemoryPasskeyStore::new();
    let origin = Url::parse(ORIGIN).expect("parse origin");
    let user_id = Uuid::new_v4();
    let mut device = WebauthnAuthenticator::new(SoftPasskey::new(true));

    // ── Registration ──────────────────────────────────────────────────────────
    let (challenge, reg_state) =
        rp.start_registration(user_id, "alice", "Alice Example", &[]).expect("start registration");
    let reg_response =
        device.do_registration(origin.clone(), challenge).expect("authenticator registers");
    let passkey = rp.finish_registration(&reg_response, &reg_state).expect("finish registration");
    store.add(user_id, passkey).await.expect("store passkey");

    assert_eq!(store.list(user_id).await.unwrap().len(), 1);
    assert_eq!(store.len(), 1);

    // ── Authentication ─────────────────────────────────────────────────────────
    let credentials = store.list(user_id).await.unwrap();
    let (challenge, auth_state) =
        rp.start_authentication(&credentials).expect("start authentication");
    let auth_response =
        device.do_authentication(origin, challenge).expect("authenticator authenticates");
    let result =
        rp.finish_authentication(&auth_response, &auth_state).expect("finish authentication");

    // The asserted credential is the one we registered.
    assert_eq!(result.cred_id(), credentials[0].cred_id());

    // Persist the post-authentication signature counter back to the store.
    let mut updated = credentials[0].clone();
    if updated.update_credential(&result).is_some() {
        assert!(store.update(user_id, &updated).await.unwrap());
    }
}

#[tokio::test]
async fn wrong_origin_fails_registration() {
    // An RP for a different origin must reject the device's response.
    let rp = PasskeyAuthenticator::new("example.com", "https://example.com", "Other RP")
        .expect("configure RP");
    let device_origin = Url::parse(ORIGIN).expect("parse origin");
    let mut device = WebauthnAuthenticator::new(SoftPasskey::new(true));

    let (challenge, reg_state) =
        rp.start_registration(Uuid::new_v4(), "bob", "Bob", &[]).expect("start registration");
    // The software authenticator signs over the (mismatched) origin it is told.
    let Ok(reg_response) = device.do_registration(device_origin, challenge) else {
        return; // device refused outright — also an acceptable rejection
    };
    assert!(
        rp.finish_registration(&reg_response, &reg_state).is_err(),
        "registration from a mismatched origin must not verify"
    );
}
