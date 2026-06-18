//! WebAuthn passkey HTTP endpoints (`webauthn` feature).
//!
//! Exposes the two `klauthed-security` ceremonies over HTTP as four `POST`
//! routes, each a server↔browser round-trip:
//!
//! * `register/start` → `register/finish` — enroll a passkey for a user.
//! * `login/start` → `login/finish` — authenticate with a registered passkey.
//!
//! Mount a [`PasskeyApi`] (which carries the [`PasskeyAuthenticator`], a
//! [`PasskeyStore`] for credentials, and a [`CeremonyStore`] for in-flight state)
//! under a scope:
//!
//! ```no_run
//! use std::sync::Arc;
//! use actix_web::{App, web};
//! use klauthed_security::passkey::{InMemoryPasskeyStore, PasskeyAuthenticator};
//! use klauthed_web::passkey::{InMemoryCeremonyStore, PasskeyApi};
//!
//! # fn build() -> Result<(), Box<dyn std::error::Error>> {
//! let api = PasskeyApi::new(
//!     Arc::new(PasskeyAuthenticator::new("example.com", "https://example.com", "Example")?),
//!     Arc::new(InMemoryPasskeyStore::new()),
//!     Arc::new(InMemoryCeremonyStore::new()),
//! );
//! let app = App::new().service(web::scope("/passkey").configure(|cfg| api.configure(cfg)));
//! # let _ = app;
//! # Ok(())
//! # }
//! ```
//!
//! What happens *after* a successful authentication — issuing a session or token
//! — is the service's job: `login/finish` returns the verified user handle. The
//! routes are unauthenticated; put `register/*` behind your own auth as needed.
//! The [`InMemoryCeremonyStore`] is for tests/dev; back it with Redis (with a
//! TTL) in production.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use actix_web::{HttpResponse, web};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use klauthed_security::passkey::{
    CreationChallengeResponse, PasskeyAuthentication, PasskeyAuthenticator, PasskeyRegistration,
    PasskeyStore, PublicKeyCredential, RegisterPublicKeyCredential, RequestChallengeResponse, Uuid,
};
use klauthed_security::random_token;

use crate::error::AppError;

/// Async storage for **in-flight** ceremony state, parked between the `start` and
/// `finish` round-trips and keyed by an opaque ceremony id.
///
/// The seam between the HTTP handlers and whatever backend holds the short-lived
/// state. Values are single-use: [`take`](Self::take) fetches **and removes**.
/// Production backends (e.g. Redis) should expire entries after a few minutes.
#[async_trait]
pub trait CeremonyStore: Send + Sync {
    /// Persist `value` (opaque serialized ceremony state) under `id`.
    ///
    /// # Errors
    /// Returns [`AppError`] only on backend failure.
    async fn put(&self, id: &str, value: String) -> Result<(), AppError>;

    /// Fetch and remove the value stored for `id` (ceremonies are single-use).
    ///
    /// # Errors
    /// Returns [`AppError`] only on backend failure.
    async fn take(&self, id: &str) -> Result<Option<String>, AppError>;
}

/// A thread-safe, in-memory [`CeremonyStore`] for tests and development.
///
/// Cloneable handles share one backing map; entries never expire, so use a
/// TTL-capable backend (Redis) in production.
#[derive(Debug, Default, Clone)]
pub struct InMemoryCeremonyStore {
    by_id: Arc<Mutex<HashMap<String, String>>>,
}

impl InMemoryCeremonyStore {
    /// Create an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl CeremonyStore for InMemoryCeremonyStore {
    async fn put(&self, id: &str, value: String) -> Result<(), AppError> {
        self.by_id.lock().unwrap_or_else(PoisonError::into_inner).insert(id.to_owned(), value);
        Ok(())
    }

    async fn take(&self, id: &str) -> Result<Option<String>, AppError> {
        Ok(self.by_id.lock().unwrap_or_else(PoisonError::into_inner).remove(id))
    }
}

/// The mountable passkey endpoints, carrying the relying party and its stores.
///
/// Cheap to clone (everything is shared behind `Arc`); build once and let actix
/// clone it per worker.
#[derive(Clone)]
pub struct PasskeyApi {
    authenticator: Arc<PasskeyAuthenticator>,
    credentials: Arc<dyn PasskeyStore>,
    ceremonies: Arc<dyn CeremonyStore>,
}

impl PasskeyApi {
    /// Build the API from a configured relying party, a credential store, and a
    /// ceremony-state store.
    #[must_use]
    pub fn new(
        authenticator: Arc<PasskeyAuthenticator>,
        credentials: Arc<dyn PasskeyStore>,
        ceremonies: Arc<dyn CeremonyStore>,
    ) -> Self {
        Self { authenticator, credentials, ceremonies }
    }

    /// Register the four `POST` routes (`register/start`, `register/finish`,
    /// `login/start`, `login/finish`) and the shared state on an app or scope.
    pub fn configure(&self, cfg: &mut web::ServiceConfig) {
        cfg.app_data(web::Data::new(Arc::clone(&self.authenticator)));
        cfg.app_data(web::Data::new(Arc::clone(&self.credentials)));
        cfg.app_data(web::Data::new(Arc::clone(&self.ceremonies)));
        cfg.route("/register/start", web::post().to(register_start));
        cfg.route("/register/finish", web::post().to(register_finish));
        cfg.route("/login/start", web::post().to(login_start));
        cfg.route("/login/finish", web::post().to(login_finish));
    }
}

// ── Wire DTOs ─────────────────────────────────────────────────────────────────

/// Body of `register/start`.
#[derive(Debug, Clone, Deserialize)]
pub struct RegisterStartRequest {
    /// Existing WebAuthn user handle to add a passkey to; omit to enroll a new one.
    #[serde(default)]
    pub user_id: Option<Uuid>,
    /// The user's account name (shown by some authenticators).
    pub user_name: String,
    /// A human-readable display name.
    pub display_name: String,
}

/// Response of `register/start`: the challenge for `navigator.credentials.create()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterStartResponse {
    /// Opaque id correlating this ceremony's `finish` call.
    pub ceremony_id: String,
    /// The WebAuthn user handle the passkey will be registered under.
    pub user_id: Uuid,
    /// The credential-creation options to pass to the browser.
    pub public_key: CreationChallengeResponse,
}

/// Body of `register/finish`.
#[derive(Debug, Clone, Deserialize)]
pub struct RegisterFinishRequest {
    /// The `ceremony_id` from `register/start`.
    pub ceremony_id: String,
    /// The browser's `navigator.credentials.create()` response.
    pub credential: RegisterPublicKeyCredential,
}

/// Response of `register/finish`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterFinishResponse {
    /// The WebAuthn user handle the passkey was registered under.
    pub user_id: Uuid,
}

/// Body of `login/start`.
#[derive(Debug, Clone, Deserialize)]
pub struct LoginStartRequest {
    /// The WebAuthn user handle to authenticate.
    pub user_id: Uuid,
}

/// Response of `login/start`: the challenge for `navigator.credentials.get()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginStartResponse {
    /// Opaque id correlating this ceremony's `finish` call.
    pub ceremony_id: String,
    /// The credential-request options to pass to the browser.
    pub public_key: RequestChallengeResponse,
}

/// Body of `login/finish`.
#[derive(Debug, Clone, Deserialize)]
pub struct LoginFinishRequest {
    /// The `ceremony_id` from `login/start`.
    pub ceremony_id: String,
    /// The browser's `navigator.credentials.get()` assertion.
    pub credential: PublicKeyCredential,
}

/// Response of `login/finish`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginFinishResponse {
    /// The authenticated WebAuthn user handle.
    pub user_id: Uuid,
    /// Always `true` on a `200` (verification failures return an error status).
    pub authenticated: bool,
}

// ── Parked ceremony state (opaque to the store) ───────────────────────────────

#[derive(Serialize, Deserialize)]
struct RegistrationState {
    user_id: Uuid,
    state: PasskeyRegistration,
}

#[derive(Serialize, Deserialize)]
struct AuthenticationState {
    user_id: Uuid,
    state: PasskeyAuthentication,
}

fn parked<T: Serialize>(value: &T) -> Result<String, AppError> {
    serde_json::to_string(value).map_err(|e| AppError::internal(format!("serialize ceremony: {e}")))
}

fn unpark<T: for<'de> Deserialize<'de>>(raw: &str) -> Result<T, AppError> {
    serde_json::from_str(raw).map_err(|e| AppError::internal(format!("deserialize ceremony: {e}")))
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn register_start(
    body: web::Json<RegisterStartRequest>,
    authenticator: web::Data<Arc<PasskeyAuthenticator>>,
    credentials: web::Data<Arc<dyn PasskeyStore>>,
    ceremonies: web::Data<Arc<dyn CeremonyStore>>,
) -> Result<HttpResponse, AppError> {
    let body = body.into_inner();
    let user_id = body.user_id.unwrap_or_else(Uuid::new_v4);
    // Exclude already-registered credentials so an authenticator won't double-enroll.
    let existing = credentials.list(user_id).await?;
    let (challenge, state) = authenticator.start_registration(
        user_id,
        &body.user_name,
        &body.display_name,
        &existing,
    )?;

    let ceremony_id = random_token(16)?;
    ceremonies.put(&ceremony_id, parked(&RegistrationState { user_id, state })?).await?;
    Ok(HttpResponse::Ok().json(RegisterStartResponse {
        ceremony_id,
        user_id,
        public_key: challenge,
    }))
}

async fn register_finish(
    body: web::Json<RegisterFinishRequest>,
    authenticator: web::Data<Arc<PasskeyAuthenticator>>,
    credentials: web::Data<Arc<dyn PasskeyStore>>,
    ceremonies: web::Data<Arc<dyn CeremonyStore>>,
) -> Result<HttpResponse, AppError> {
    let body = body.into_inner();
    let raw = ceremonies
        .take(&body.ceremony_id)
        .await?
        .ok_or_else(|| AppError::bad_request("unknown or expired ceremony"))?;
    let RegistrationState { user_id, state } = unpark(&raw)?;

    let passkey = authenticator.finish_registration(&body.credential, &state)?;
    credentials.add(user_id, passkey).await?;
    Ok(HttpResponse::Ok().json(RegisterFinishResponse { user_id }))
}

async fn login_start(
    body: web::Json<LoginStartRequest>,
    authenticator: web::Data<Arc<PasskeyAuthenticator>>,
    credentials: web::Data<Arc<dyn PasskeyStore>>,
    ceremonies: web::Data<Arc<dyn CeremonyStore>>,
) -> Result<HttpResponse, AppError> {
    let user_id = body.into_inner().user_id;
    let creds = credentials.list(user_id).await?;
    if creds.is_empty() {
        return Err(AppError::not_found("no passkeys registered for user"));
    }

    let (challenge, state) = authenticator.start_authentication(&creds)?;
    let ceremony_id = random_token(16)?;
    ceremonies.put(&ceremony_id, parked(&AuthenticationState { user_id, state })?).await?;
    Ok(HttpResponse::Ok().json(LoginStartResponse { ceremony_id, public_key: challenge }))
}

async fn login_finish(
    body: web::Json<LoginFinishRequest>,
    authenticator: web::Data<Arc<PasskeyAuthenticator>>,
    credentials: web::Data<Arc<dyn PasskeyStore>>,
    ceremonies: web::Data<Arc<dyn CeremonyStore>>,
) -> Result<HttpResponse, AppError> {
    let body = body.into_inner();
    let raw = ceremonies
        .take(&body.ceremony_id)
        .await?
        .ok_or_else(|| AppError::bad_request("unknown or expired ceremony"))?;
    let AuthenticationState { user_id, state } = unpark(&raw)?;

    let result = authenticator.finish_authentication(&body.credential, &state)?;

    // Persist the updated signature counter (clone-detection) for the asserted credential.
    if let Some(mut credential) =
        credentials.list(user_id).await?.into_iter().find(|c| c.cred_id() == result.cred_id())
        && credential.update_credential(&result).is_some()
    {
        credentials.update(user_id, &credential).await?;
    }
    Ok(HttpResponse::Ok().json(LoginFinishResponse { user_id, authenticated: true }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{App, test as http_test};
    use klauthed_security::passkey::{InMemoryPasskeyStore, Url};
    use webauthn_authenticator_rs::WebauthnAuthenticator;
    use webauthn_authenticator_rs::softpasskey::SoftPasskey;

    const ORIGIN: &str = "http://localhost:8080";

    fn api() -> PasskeyApi {
        PasskeyApi::new(
            Arc::new(
                PasskeyAuthenticator::new("localhost", ORIGIN, "Test RP").expect("configure RP"),
            ),
            Arc::new(InMemoryPasskeyStore::new()),
            Arc::new(InMemoryCeremonyStore::new()),
        )
    }

    #[actix_web::test]
    async fn register_then_authenticate_over_http() {
        let app = http_test::init_service(
            App::new().service(web::scope("/passkey").configure(|cfg| api().configure(cfg))),
        )
        .await;
        let origin = Url::parse(ORIGIN).expect("parse origin");
        let mut device = WebauthnAuthenticator::new(SoftPasskey::new(true));

        // ── Registration ───────────────────────────────────────────────────────
        let req = http_test::TestRequest::post()
            .uri("/passkey/register/start")
            .set_json(serde_json::json!({ "user_name": "alice", "display_name": "Alice" }))
            .to_request();
        let start: RegisterStartResponse = http_test::call_and_read_body_json(&app, req).await;
        let reg_response =
            device.do_registration(origin.clone(), start.public_key).expect("device registers");

        let req = http_test::TestRequest::post()
            .uri("/passkey/register/finish")
            .set_json(serde_json::json!({
                "ceremony_id": start.ceremony_id,
                "credential": reg_response,
            }))
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        assert!(resp.status().is_success(), "register/finish: {}", resp.status());

        // ── Authentication ───────────────────────────────────────────────────────
        let req = http_test::TestRequest::post()
            .uri("/passkey/login/start")
            .set_json(serde_json::json!({ "user_id": start.user_id }))
            .to_request();
        let login: LoginStartResponse = http_test::call_and_read_body_json(&app, req).await;
        let auth_response =
            device.do_authentication(origin, login.public_key).expect("device authenticates");

        let req = http_test::TestRequest::post()
            .uri("/passkey/login/finish")
            .set_json(serde_json::json!({
                "ceremony_id": login.ceremony_id,
                "credential": auth_response,
            }))
            .to_request();
        let finish: LoginFinishResponse = http_test::call_and_read_body_json(&app, req).await;
        assert!(finish.authenticated);
        assert_eq!(finish.user_id, start.user_id);
    }

    #[actix_web::test]
    async fn login_start_404s_for_a_user_without_passkeys() {
        let app = http_test::init_service(
            App::new().service(web::scope("/passkey").configure(|cfg| api().configure(cfg))),
        )
        .await;

        let req = http_test::TestRequest::post()
            .uri("/passkey/login/start")
            .set_json(serde_json::json!({ "user_id": Uuid::new_v4() }))
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
    }

    #[actix_web::test]
    async fn finish_with_unknown_ceremony_is_rejected() {
        let app = http_test::init_service(
            App::new().service(web::scope("/passkey").configure(|cfg| api().configure(cfg))),
        )
        .await;

        let req = http_test::TestRequest::post()
            .uri("/passkey/register/finish")
            .set_json(serde_json::json!({
                "ceremony_id": "nope",
                "credential": serde_json::json!({}),
            }))
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        assert!(resp.status().is_client_error(), "got {}", resp.status());
    }
}
