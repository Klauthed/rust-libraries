//! OIDC UserInfo endpoint (OpenID Connect Core 1.0 §5.3) and the
//! [`UserInfoProvider`] SPI.
//!
//! The endpoint is protected by a Bearer access token (mount it behind
//! [`JwtAuth`](crate::auth::JwtAuth)). The library handles token validation,
//! the `openid`-scope requirement, and forcing `sub` to match the token; the
//! *claim values* come from your [`UserInfoProvider`] implementation, which
//! reads your user store.
//!
//! ```no_run
//! use std::sync::Arc;
//! use actix_web::{web, App};
//! use async_trait::async_trait;
//! use klauthed_protocol::oidc::StandardClaims;
//! use klauthed_security::JwtVerifier;
//! use klauthed_web::auth::JwtAuth;
//! use klauthed_web::error::AppResult;
//! use klauthed_web::oauth::userinfo::{configure as configure_userinfo, UserInfoProvider};
//!
//! struct MyUsers;
//! #[async_trait]
//! impl UserInfoProvider for MyUsers {
//!     async fn userinfo(&self, subject: &str, _scopes: &[String])
//!         -> AppResult<Option<StandardClaims>>
//!     {
//!         Ok(Some(StandardClaims { sub: Some(subject.into()), ..Default::default() }))
//!     }
//! }
//!
//! let provider: Arc<dyn UserInfoProvider> = Arc::new(MyUsers);
//! let _app = App::new()
//!     .app_data(web::Data::new(JwtVerifier::hs256(b"secret")))
//!     .app_data(web::Data::from(provider))
//!     .wrap(JwtAuth::new())
//!     .configure(configure_userinfo);
//! ```

use actix_web::{web, HttpResponse, ResponseError as _};
use async_trait::async_trait;
use klauthed_protocol::oidc::StandardClaims;

use crate::auth::AuthenticatedUser;
use crate::error::{AppError, AppResult};

/// SPI: resolves the OIDC claims for a subject from the app's user store.
///
/// Implement this against your data layer. It is the only app-specific part of
/// the UserInfo endpoint — the library handles token validation and response
/// shaping.
///
/// `scopes` are the scopes granted to the presented access token; honour them
/// when deciding which claims to release (e.g. only include `email` /
/// `email_verified` when the `email` scope is present). The `sub` you return is
/// overwritten with the token's subject, so it is always consistent.
#[async_trait]
pub trait UserInfoProvider: Send + Sync + 'static {
    /// Return the claims for `subject`, limited to what `scopes` permit.
    ///
    /// Return `Ok(None)` when the subject no longer exists (yields `404`);
    /// return `Err` for backend failures (yields the mapped error status).
    async fn userinfo(
        &self,
        subject: &str,
        scopes: &[String],
    ) -> AppResult<Option<StandardClaims>>;
}

/// `GET`/`POST` `/oauth/userinfo`
///
/// Requires a valid Bearer access token (via [`JwtAuth`](crate::auth::JwtAuth))
/// carrying the `openid` scope. Returns the subject's [`StandardClaims`] as
/// JSON, with `sub` guaranteed to equal the access token's subject.
async fn userinfo_handler(
    user: AuthenticatedUser,
    provider: web::Data<dyn UserInfoProvider>,
) -> HttpResponse {
    // The access token must have been granted the `openid` scope (OIDC §5.3.1).
    if !user.has_scope("openid") {
        return AppError::forbidden("access token is missing the required `openid` scope")
            .error_response();
    }

    let Some(subject) = user.sub() else {
        return AppError::unauthorized("access token has no subject").error_response();
    };
    let subject = subject.to_owned();
    let scopes: Vec<String> = user.scopes().into_iter().map(str::to_owned).collect();

    match provider.userinfo(&subject, &scopes).await {
        Ok(Some(mut claims)) => {
            // `sub` MUST match the access token's subject (OIDC Core §5.3.2).
            claims.sub = Some(subject);
            HttpResponse::Ok().json(claims)
        }
        Ok(None) => AppError::not_found("user not found").error_response(),
        Err(e) => e.error_response(),
    }
}

/// Mount the UserInfo route on an app.
///
/// Routes added: `GET /oauth/userinfo` and `POST /oauth/userinfo` (OIDC
/// permits both). Register a `UserInfoProvider` via
/// `app_data(web::Data::from(provider))` and a `JwtVerifier`, and wrap the
/// scope with [`JwtAuth`](crate::auth::JwtAuth).
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/oauth/userinfo", web::get().to(userinfo_handler))
        .route("/oauth/userinfo", web::post().to(userinfo_handler));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use actix_web::http::StatusCode;
    use actix_web::test as http_test;
    use actix_web::{web, App};
    use klauthed_core::time::{Duration, SystemClock};
    use klauthed_security::jwt::{Claims, JwtSigner};
    use klauthed_security::JwtVerifier;

    use crate::auth::JwtAuth;

    const SECRET: &[u8] = b"userinfo-test-secret";

    struct StaticProvider;

    #[async_trait]
    impl UserInfoProvider for StaticProvider {
        async fn userinfo(
            &self,
            subject: &str,
            scopes: &[String],
        ) -> AppResult<Option<StandardClaims>> {
            let email = scopes
                .iter()
                .any(|s| s == "email")
                .then(|| "alice@example.com".to_owned());
            Ok(Some(StandardClaims {
                sub: Some(subject.to_owned()),
                name: Some("Alice".into()),
                email,
                ..Default::default()
            }))
        }
    }

    /// Mint an access token carrying `scope`.
    fn access_token(sub: &str, scope: &str) -> String {
        JwtSigner::hs256(SECRET)
            .encode(
                &Claims::builder(sub, &SystemClock, Duration::hours(1))
                    .claim("scope", scope)
                    .build(),
            )
            .unwrap()
    }

    macro_rules! userinfo_app {
        () => {{
            let provider: Arc<dyn UserInfoProvider> = Arc::new(StaticProvider);
            http_test::init_service(
                App::new()
                    .app_data(web::Data::new(JwtVerifier::hs256(SECRET)))
                    .app_data(web::Data::from(provider))
                    .wrap(JwtAuth::new())
                    .configure(configure),
            )
            .await
        }};
    }

    #[actix_web::test]
    async fn returns_claims_for_openid_token() {
        let app = userinfo_app!();
        let token = access_token("alice", "openid email");
        let req = http_test::TestRequest::get()
            .uri("/oauth/userinfo")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body = http_test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["sub"], "alice");
        assert_eq!(json["name"], "Alice");
        assert_eq!(json["email"], "alice@example.com");
    }

    #[actix_web::test]
    async fn email_omitted_without_email_scope() {
        let app = userinfo_app!();
        let token = access_token("alice", "openid");
        let req = http_test::TestRequest::get()
            .uri("/oauth/userinfo")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body = http_test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["sub"], "alice");
        assert!(json.get("email").is_none());
    }

    #[actix_web::test]
    async fn rejects_token_without_openid_scope() {
        let app = userinfo_app!();
        let token = access_token("alice", "profile email");
        let req = http_test::TestRequest::get()
            .uri("/oauth/userinfo")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[actix_web::test]
    async fn rejects_missing_token() {
        let app = userinfo_app!();
        let req = http_test::TestRequest::get()
            .uri("/oauth/userinfo")
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        // JwtAuth rejects before the handler runs.
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}
