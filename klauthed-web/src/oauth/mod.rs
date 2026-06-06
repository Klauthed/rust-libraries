//! OAuth 2.0 authorization code flow endpoints (RFC 6749 §4.1).
//!
//! # Structure
//!
//! | Sub-module | Responsibility |
//! |---|---|
//! | [`config`] | [`OAuthConfig`] and its builder |
//! | `util` (internal) | Redirect URL construction and OAuth error responses |
//! | [`handlers`] | `/oauth/authorize` and `/oauth/token` handler functions |
//! | [`discovery`] | `/.well-known/openid-configuration` |
//! | [`jwks`] | `/oauth/jwks` — publishes app-registered public keys |
//! | [`userinfo`] | `/oauth/userinfo` + the [`UserInfoProvider`](userinfo::UserInfoProvider) SPI |
//!
//! # Wiring
//!
//! ```no_run
//! use std::sync::Arc;
//! use actix_web::{web, App};
//! use klauthed_security::{JwtVerifier, InMemoryClientStore, InMemoryAuthCodeStore, JwtSigner};
//! use klauthed_web::auth::JwtAuth;
//! use klauthed_web::oauth::{OAuthConfig, configure as configure_oauth};
//!
//! let config = OAuthConfig::builder()
//!     .client_store(Arc::new(InMemoryClientStore::new()))
//!     .code_store(Arc::new(InMemoryAuthCodeStore::new()))
//!     .signer(JwtSigner::hs256(b"signing-secret"))
//!     .issuer("https://auth.example.com")
//!     .build();
//!
//! let _app = App::new()
//!     .app_data(web::Data::new(JwtVerifier::hs256(b"signing-secret")))
//!     .app_data(web::Data::new(config))
//!     .wrap(JwtAuth::new())
//!     .configure(configure_oauth);
//! ```

pub mod config;
pub mod discovery;
pub mod handlers;
pub mod jwks;
pub mod userinfo;
pub(super) mod util;

pub use config::{OAuthConfig, OAuthConfigBuilder};
pub use userinfo::UserInfoProvider;

use actix_web::web;

/// Mount the OAuth 2.0 endpoints on an app or scope.
///
/// Routes added:
/// * `GET  /oauth/authorize` — authorization endpoint (requires [`JwtAuth`](crate::auth::JwtAuth))
/// * `POST /oauth/token`     — token endpoint
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/oauth/authorize", web::get().to(handlers::authorize))
        .route("/oauth/token", web::post().to(handlers::token));
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use actix_web::http::StatusCode;
    use actix_web::test as http_test;
    use actix_web::{web, App};
    use klauthed_core::time::{Duration, FixedClock, Timestamp};
    use klauthed_security::{
        authz_code::{AuthCodeBuilder, PkceMethod},
        oauth2_client::{ClientGrantType, ClientType, InMemoryClientStore, OAuth2Client,
            TokenEndpointAuthMethod},
        InMemoryAuthCodeStore, JwtSigner, JwtVerifier,
    };
    use klauthed_core::time::Clock;

    use crate::auth::JwtAuth;

    const SECRET: &[u8] = b"test-secret";

    fn make_config(clock: Arc<dyn Clock>) -> OAuthConfig {
        OAuthConfig::builder()
            .client_store(Arc::new(InMemoryClientStore::new()))
            .code_store(Arc::new(InMemoryAuthCodeStore::with_clock(clock.clone())))
            .signer(JwtSigner::hs256(SECRET))
            .issuer("https://test.example.com")
            .clock(clock)
            .build()
    }

    fn test_client(id: &str) -> OAuth2Client {
        OAuth2Client {
            client_id: id.into(),
            client_type: ClientType::Public,
            client_secret_hash: None,
            redirect_uris: vec!["https://app.example.com/cb".into()],
            allowed_grant_types: [ClientGrantType::AuthorizationCode].into_iter().collect(),
            allowed_scopes: ["openid", "email"].iter().map(|s| s.to_string()).collect(),
            token_endpoint_auth_method: TokenEndpointAuthMethod::None,
            client_name: None,
            created_at: Timestamp::now(),
        }
    }

    fn user_token(sub: &str) -> String {
        JwtSigner::hs256(SECRET)
            .encode(
                &klauthed_security::Claims::builder(
                    sub,
                    &klauthed_core::time::SystemClock,
                    Duration::hours(1),
                )
                .build(),
            )
            .unwrap()
    }

    fn s256_challenge(verifier: &str) -> String {
        use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
        use ring::digest;
        let hash = digest::digest(&digest::SHA256, verifier.as_bytes());
        URL_SAFE_NO_PAD.encode(hash.as_ref())
    }

    // ── /oauth/authorize ──────────────────────────────────────────────────────

    #[actix_web::test]
    async fn authorize_issues_code_and_redirects() {
        let clock = Arc::new(FixedClock::at_unix_millis(0));
        let config = make_config(clock);
        config.client_store.register(test_client("c1")).await.unwrap();

        let app = http_test::init_service(
            App::new()
                .app_data(web::Data::new(JwtVerifier::hs256(SECRET)))
                .app_data(web::Data::new(config))
                .wrap(JwtAuth::new())
                .configure(configure),
        )
        .await;

        let req = http_test::TestRequest::get()
            .uri("/oauth/authorize?response_type=code&client_id=c1\
                  &redirect_uri=https%3A%2F%2Fapp.example.com%2Fcb\
                  &scope=openid%20email&state=xyz")
            .insert_header(("Authorization", format!("Bearer {}", user_token("alice"))))
            .to_request();

        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::FOUND);
        let location = resp.headers().get("Location").unwrap().to_str().unwrap();
        assert!(location.starts_with("https://app.example.com/cb?code="));
        assert!(location.contains("state=xyz"));
    }

    #[actix_web::test]
    async fn authorize_rejects_unknown_client() {
        let clock = Arc::new(FixedClock::at_unix_millis(0));
        let config = make_config(clock);

        let app = http_test::init_service(
            App::new()
                .app_data(web::Data::new(JwtVerifier::hs256(SECRET)))
                .app_data(web::Data::new(config))
                .wrap(JwtAuth::new())
                .configure(configure),
        )
        .await;

        let req = http_test::TestRequest::get()
            .uri("/oauth/authorize?response_type=code&client_id=UNKNOWN\
                  &redirect_uri=https%3A%2F%2Fapp.example.com%2Fcb")
            .insert_header(("Authorization", format!("Bearer {}", user_token("alice"))))
            .to_request();

        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[actix_web::test]
    async fn authorize_rejects_invalid_redirect_uri() {
        let clock = Arc::new(FixedClock::at_unix_millis(0));
        let config = make_config(clock);
        config.client_store.register(test_client("c1")).await.unwrap();

        let app = http_test::init_service(
            App::new()
                .app_data(web::Data::new(JwtVerifier::hs256(SECRET)))
                .app_data(web::Data::new(config))
                .wrap(JwtAuth::new())
                .configure(configure),
        )
        .await;

        let req = http_test::TestRequest::get()
            .uri("/oauth/authorize?response_type=code&client_id=c1\
                  &redirect_uri=https%3A%2F%2Fevil.example.com%2Fcb")
            .insert_header(("Authorization", format!("Bearer {}", user_token("alice"))))
            .to_request();

        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[actix_web::test]
    async fn authorize_error_for_invalid_scope_redirects_with_error() {
        let clock = Arc::new(FixedClock::at_unix_millis(0));
        let config = make_config(clock);
        config.client_store.register(test_client("c1")).await.unwrap();

        let app = http_test::init_service(
            App::new()
                .app_data(web::Data::new(JwtVerifier::hs256(SECRET)))
                .app_data(web::Data::new(config))
                .wrap(JwtAuth::new())
                .configure(configure),
        )
        .await;

        let req = http_test::TestRequest::get()
            .uri("/oauth/authorize?response_type=code&client_id=c1\
                  &redirect_uri=https%3A%2F%2Fapp.example.com%2Fcb\
                  &scope=openid%20admin")
            .insert_header(("Authorization", format!("Bearer {}", user_token("alice"))))
            .to_request();

        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::FOUND);
        let location = resp.headers().get("Location").unwrap().to_str().unwrap();
        assert!(location.contains("error=invalid_scope"));
    }

    // ── /oauth/token ──────────────────────────────────────────────────────────

    #[actix_web::test]
    async fn token_exchange_returns_access_token() {
        let clock = Arc::new(FixedClock::new(Timestamp::now()));
        let config = make_config(clock.clone());
        config.client_store.register(test_client("c2")).await.unwrap();

        let code = AuthCodeBuilder::new("c2", "bob")
            .redirect_uri("https://app.example.com/cb")
            .scope(vec!["openid".into()])
            .build(&*clock, Duration::minutes(5))
            .unwrap();
        let code_str = code.code.clone();
        config.code_store.store(code).await.unwrap();

        let app = http_test::init_service(
            App::new()
                .app_data(web::Data::new(JwtVerifier::hs256(SECRET)))
                .app_data(web::Data::new(config))
                .configure(configure),
        )
        .await;

        let form = format!(
            "grant_type=authorization_code&code={code_str}\
             &client_id=c2&redirect_uri=https%3A%2F%2Fapp.example.com%2Fcb"
        );
        let req = http_test::TestRequest::post()
            .uri("/oauth/token")
            .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
            .set_payload(form)
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body = http_test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(!json["access_token"].as_str().unwrap().is_empty());
        assert_eq!(json["token_type"], "Bearer");
        assert_eq!(json["expires_in"], 3600);

        let decoded = JwtVerifier::hs256(SECRET)
            .expecting_issuer("https://test.example.com")
            .decode(json["access_token"].as_str().unwrap())
            .unwrap();
        assert_eq!(decoded.sub.as_deref(), Some("bob"));
    }

    #[actix_web::test]
    async fn auth_code_exchange_with_openid_scope_returns_id_token() {
        let clock = Arc::new(FixedClock::new(Timestamp::now()));
        let config = make_config(clock.clone());
        config.client_store.register(test_client("c-oidc")).await.unwrap();

        let code = AuthCodeBuilder::new("c-oidc", "bob")
            .redirect_uri("https://app.example.com/cb")
            .scope(vec!["openid".into()])
            .nonce("n-123")
            .build(&*clock, Duration::minutes(5))
            .unwrap();
        let code_str = code.code.clone();
        config.code_store.store(code).await.unwrap();

        let app = http_test::init_service(
            App::new()
                .app_data(web::Data::new(JwtVerifier::hs256(SECRET)))
                .app_data(web::Data::new(config))
                .configure(configure),
        )
        .await;

        let form = format!(
            "grant_type=authorization_code&code={code_str}\
             &client_id=c-oidc&redirect_uri=https%3A%2F%2Fapp.example.com%2Fcb"
        );
        let req = http_test::TestRequest::post()
            .uri("/oauth/token")
            .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
            .set_payload(form)
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body = http_test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let id_token = json["id_token"].as_str().expect("id_token present");
        assert!(!id_token.is_empty());

        // The ID token's audience is the client; consumers must validate it.
        let decoded = JwtVerifier::hs256(SECRET)
            .expecting_issuer("https://test.example.com")
            .expecting_audience("c-oidc")
            .decode(id_token)
            .unwrap();
        assert_eq!(decoded.sub.as_deref(), Some("bob"));
        assert_eq!(decoded.aud.as_deref(), Some("c-oidc"));
        assert_eq!(
            decoded.custom.get("nonce").and_then(|v| v.as_str()),
            Some("n-123")
        );
    }

    #[actix_web::test]
    async fn auth_code_exchange_without_openid_scope_omits_id_token() {
        let clock = Arc::new(FixedClock::new(Timestamp::now()));
        let config = make_config(clock.clone());
        config.client_store.register(test_client("c-no-oidc")).await.unwrap();

        let code = AuthCodeBuilder::new("c-no-oidc", "bob")
            .redirect_uri("https://app.example.com/cb")
            .scope(vec!["email".into()])
            .build(&*clock, Duration::minutes(5))
            .unwrap();
        let code_str = code.code.clone();
        config.code_store.store(code).await.unwrap();

        let app = http_test::init_service(
            App::new()
                .app_data(web::Data::new(JwtVerifier::hs256(SECRET)))
                .app_data(web::Data::new(config))
                .configure(configure),
        )
        .await;

        let form = format!(
            "grant_type=authorization_code&code={code_str}\
             &client_id=c-no-oidc&redirect_uri=https%3A%2F%2Fapp.example.com%2Fcb"
        );
        let req = http_test::TestRequest::post()
            .uri("/oauth/token")
            .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
            .set_payload(form)
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body = http_test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        // No `openid` scope → no ID token.
        assert!(json["id_token"].is_null());
    }

    #[actix_web::test]
    async fn token_exchange_rejects_replayed_code() {
        let clock = Arc::new(FixedClock::at_unix_millis(0));
        let config = make_config(clock.clone());
        config.client_store.register(test_client("c3")).await.unwrap();

        let code = AuthCodeBuilder::new("c3", "carol")
            .redirect_uri("https://app.example.com/cb")
            .build(&*clock, Duration::minutes(5))
            .unwrap();
        let code_str = code.code.clone();
        config.code_store.store(code).await.unwrap();

        let app = http_test::init_service(
            App::new()
                .app_data(web::Data::new(JwtVerifier::hs256(SECRET)))
                .app_data(web::Data::new(config))
                .configure(configure),
        )
        .await;

        let form = format!(
            "grant_type=authorization_code&code={code_str}\
             &client_id=c3&redirect_uri=https%3A%2F%2Fapp.example.com%2Fcb"
        );

        let req1 = http_test::TestRequest::post()
            .uri("/oauth/token")
            .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
            .set_payload(form.clone())
            .to_request();
        let resp1 = http_test::call_service(&app, req1).await;
        assert_eq!(resp1.status(), StatusCode::OK);

        let req2 = http_test::TestRequest::post()
            .uri("/oauth/token")
            .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
            .set_payload(form)
            .to_request();
        let resp2 = http_test::call_service(&app, req2).await;
        assert_eq!(resp2.status(), StatusCode::BAD_REQUEST);
        let body = http_test::read_body(resp2).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"], "invalid_grant");
    }

    #[actix_web::test]
    async fn token_exchange_with_pkce_s256_succeeds() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = s256_challenge(verifier);
        let clock = Arc::new(FixedClock::at_unix_millis(0));
        let config = make_config(clock.clone());
        config.client_store.register(test_client("pkce-ok")).await.unwrap();

        let code = AuthCodeBuilder::new("pkce-ok", "dave")
            .redirect_uri("https://app.example.com/cb")
            .pkce(&challenge, PkceMethod::S256)
            .build(&*clock, Duration::minutes(5))
            .unwrap();
        let code_str = code.code.clone();
        config.code_store.store(code).await.unwrap();

        let app = http_test::init_service(
            App::new()
                .app_data(web::Data::new(JwtVerifier::hs256(SECRET)))
                .app_data(web::Data::new(config))
                .configure(configure),
        )
        .await;

        let form = format!(
            "grant_type=authorization_code&code={code_str}\
             &client_id=pkce-ok&redirect_uri=https%3A%2F%2Fapp.example.com%2Fcb\
             &code_verifier={verifier}"
        );
        let req = http_test::TestRequest::post()
            .uri("/oauth/token")
            .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
            .set_payload(form)
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[actix_web::test]
    async fn token_exchange_with_wrong_pkce_verifier_fails() {
        let real_verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = s256_challenge(real_verifier);
        let clock = Arc::new(FixedClock::at_unix_millis(0));
        let config = make_config(clock.clone());
        config.client_store.register(test_client("pkce-bad")).await.unwrap();

        let code = AuthCodeBuilder::new("pkce-bad", "eve")
            .redirect_uri("https://app.example.com/cb")
            .pkce(&challenge, PkceMethod::S256)
            .build(&*clock, Duration::minutes(5))
            .unwrap();
        let code_str = code.code.clone();
        config.code_store.store(code).await.unwrap();

        let app = http_test::init_service(
            App::new()
                .app_data(web::Data::new(JwtVerifier::hs256(SECRET)))
                .app_data(web::Data::new(config))
                .configure(configure),
        )
        .await;

        let form = format!(
            "grant_type=authorization_code&code={code_str}\
             &client_id=pkce-bad&redirect_uri=https%3A%2F%2Fapp.example.com%2Fcb\
             &code_verifier=wrong-verifier"
        );
        let req = http_test::TestRequest::post()
            .uri("/oauth/token")
            .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
            .set_payload(form)
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = http_test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"], "invalid_grant");
    }

    // ── refresh_token grant ───────────────────────────────────────────────────

    fn test_client_with_refresh(id: &str) -> OAuth2Client {
        OAuth2Client {
            client_id: id.into(),
            client_type: ClientType::Public,
            client_secret_hash: None,
            redirect_uris: vec!["https://app.example.com/cb".into()],
            allowed_grant_types: [
                ClientGrantType::AuthorizationCode,
                ClientGrantType::RefreshToken,
            ]
            .into_iter()
            .collect(),
            allowed_scopes: ["openid", "email"].iter().map(|s| s.to_string()).collect(),
            token_endpoint_auth_method: TokenEndpointAuthMethod::None,
            client_name: None,
            created_at: Timestamp::now(),
        }
    }

    fn make_config_with_refresh(clock: Arc<dyn Clock>) -> OAuthConfig {
        use klauthed_security::InMemoryRefreshTokenStore;
        OAuthConfig::builder()
            .client_store(Arc::new(InMemoryClientStore::new()))
            .code_store(Arc::new(InMemoryAuthCodeStore::with_clock(clock.clone())))
            .refresh_token_store(Arc::new(InMemoryRefreshTokenStore::with_clock(clock.clone())))
            .signer(JwtSigner::hs256(SECRET))
            .issuer("https://test.example.com")
            .clock(clock)
            .build()
    }

    #[actix_web::test]
    async fn auth_code_exchange_issues_refresh_token_when_store_configured() {
        let clock = Arc::new(FixedClock::new(klauthed_core::time::Timestamp::now()));
        let config = make_config_with_refresh(clock.clone());
        config.client_store.register(test_client_with_refresh("c-rt")).await.unwrap();

        let code = AuthCodeBuilder::new("c-rt", "alice")
            .redirect_uri("https://app.example.com/cb")
            .scope(vec!["openid".into()])
            .build(&*clock, Duration::minutes(5))
            .unwrap();
        let code_str = code.code.clone();
        config.code_store.store(code).await.unwrap();

        let app = http_test::init_service(
            App::new()
                .app_data(web::Data::new(JwtVerifier::hs256(SECRET)))
                .app_data(web::Data::new(config))
                .configure(configure),
        )
        .await;

        let form = format!(
            "grant_type=authorization_code&code={code_str}\
             &client_id=c-rt&redirect_uri=https%3A%2F%2Fapp.example.com%2Fcb"
        );
        let req = http_test::TestRequest::post()
            .uri("/oauth/token")
            .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
            .set_payload(form)
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body = http_test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(!json["access_token"].as_str().unwrap().is_empty());
        // A refresh token must be present when the store is configured.
        assert!(!json["refresh_token"].as_str().unwrap_or("").is_empty());
    }

    #[actix_web::test]
    async fn refresh_token_grant_rotates_tokens() {
        let clock = Arc::new(FixedClock::new(klauthed_core::time::Timestamp::now()));
        let config = make_config_with_refresh(clock.clone());
        config.client_store.register(test_client_with_refresh("c-rot")).await.unwrap();

        // Directly insert a refresh token into the store.
        use klauthed_security::refresh_token::RefreshTokenBuilder;
        let rt = RefreshTokenBuilder::new("c-rot", "bob")
            .scope(vec!["openid".into()])
            .build(&*clock, Duration::days(30))
            .unwrap();
        let rt_str = rt.token.clone();
        config.refresh_token_store.as_ref().unwrap().store(rt).await.unwrap();

        let app = http_test::init_service(
            App::new()
                .app_data(web::Data::new(JwtVerifier::hs256(SECRET)))
                .app_data(web::Data::new(config))
                .configure(configure),
        )
        .await;

        let form = format!("grant_type=refresh_token&refresh_token={rt_str}&client_id=c-rot");
        let req = http_test::TestRequest::post()
            .uri("/oauth/token")
            .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
            .set_payload(form)
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body = http_test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(!json["access_token"].as_str().unwrap().is_empty());
        // A new (rotated) refresh token must be returned.
        let new_rt = json["refresh_token"].as_str().unwrap_or("");
        assert!(!new_rt.is_empty());
        // The new token must be different from the consumed one.
        assert_ne!(new_rt, rt_str);
    }

    #[actix_web::test]
    async fn refresh_grant_with_openid_scope_returns_id_token() {
        let clock = Arc::new(FixedClock::new(Timestamp::now()));
        let config = make_config_with_refresh(clock.clone());
        config.client_store.register(test_client_with_refresh("c-rt-oidc")).await.unwrap();

        use klauthed_security::refresh_token::RefreshTokenBuilder;
        let rt = RefreshTokenBuilder::new("c-rt-oidc", "bob")
            .scope(vec!["openid".into()])
            .build(&*clock, Duration::days(30))
            .unwrap();
        let rt_str = rt.token.clone();
        config.refresh_token_store.as_ref().unwrap().store(rt).await.unwrap();

        let app = http_test::init_service(
            App::new()
                .app_data(web::Data::new(JwtVerifier::hs256(SECRET)))
                .app_data(web::Data::new(config))
                .configure(configure),
        )
        .await;

        let form = format!("grant_type=refresh_token&refresh_token={rt_str}&client_id=c-rt-oidc");
        let req = http_test::TestRequest::post()
            .uri("/oauth/token")
            .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
            .set_payload(form)
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body = http_test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let id_token = json["id_token"].as_str().expect("id_token present");
        let decoded = JwtVerifier::hs256(SECRET)
            .expecting_issuer("https://test.example.com")
            .expecting_audience("c-rt-oidc")
            .decode(id_token)
            .unwrap();
        assert_eq!(decoded.sub.as_deref(), Some("bob"));
    }

    #[actix_web::test]
    async fn replay_refresh_token_returns_invalid_grant() {
        let clock = Arc::new(FixedClock::new(klauthed_core::time::Timestamp::now()));
        let config = make_config_with_refresh(clock.clone());
        config.client_store.register(test_client_with_refresh("c-rep")).await.unwrap();

        use klauthed_security::refresh_token::RefreshTokenBuilder;
        let rt = RefreshTokenBuilder::new("c-rep", "carol")
            .scope(vec!["openid".into()])
            .build(&*clock, Duration::days(30))
            .unwrap();
        let rt_str = rt.token.clone();
        config.refresh_token_store.as_ref().unwrap().store(rt).await.unwrap();

        let app = http_test::init_service(
            App::new()
                .app_data(web::Data::new(JwtVerifier::hs256(SECRET)))
                .app_data(web::Data::new(config))
                .configure(configure),
        )
        .await;

        let form = format!("grant_type=refresh_token&refresh_token={rt_str}&client_id=c-rep");

        // First use: valid.
        let req1 = http_test::TestRequest::post()
            .uri("/oauth/token")
            .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
            .set_payload(form.clone())
            .to_request();
        assert_eq!(http_test::call_service(&app, req1).await.status(), StatusCode::OK);

        // Replay: invalid_grant (and family is revoked).
        let req2 = http_test::TestRequest::post()
            .uri("/oauth/token")
            .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
            .set_payload(form)
            .to_request();
        let resp2 = http_test::call_service(&app, req2).await;
        assert_eq!(resp2.status(), StatusCode::BAD_REQUEST);
        let body = http_test::read_body(resp2).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"], "invalid_grant");
    }
}
