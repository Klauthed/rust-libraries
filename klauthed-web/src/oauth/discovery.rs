//! OIDC Provider Discovery endpoint (OpenID Connect Discovery 1.0).
//!
//! Serves the standard `GET /.well-known/openid-configuration` document so
//! OAuth 2.0 / OIDC clients can auto-configure themselves.
//!
//! # Usage
//!
//! Build a [`ProviderMetadata`] document — either manually or via the
//! convenience [`build_metadata`] helper — register it as app data, and mount
//! [`configure`]:
//!
//! ```no_run
//! use actix_web::{web, App};
//! use klauthed_web::oauth::{OAuthConfig, configure as configure_oauth};
//! use klauthed_web::oauth::discovery::{self, configure as configure_discovery};
//!
//! # let oauth_config: OAuthConfig = todo!();
//! let meta = discovery::build_metadata(&oauth_config);
//!
//! let _app = App::new()
//!     .app_data(web::Data::new(meta))
//!     .configure(configure_discovery)
//!     .configure(configure_oauth);
//! ```

use actix_web::{web, HttpResponse};
use klauthed_protocol::oidc::{GrantType, ProviderMetadata, ResponseType, SubjectType};

use super::config::OAuthConfig;

// ── Discovery handler ─────────────────────────────────────────────────────────

/// `GET /.well-known/openid-configuration`
///
/// Returns the [`ProviderMetadata`] JSON document registered as app data.
/// If no document is registered the response is `404`.
async fn discovery_handler(meta: Option<web::Data<ProviderMetadata>>) -> HttpResponse {
    match meta {
        Some(m) => HttpResponse::Ok()
            .content_type("application/json")
            .json(m.as_ref()),
        None => HttpResponse::NotFound().finish(),
    }
}

/// Mount the OIDC discovery route on an app.
///
/// Route added: `GET /.well-known/openid-configuration`
///
/// Register a [`ProviderMetadata`] document via
/// `app_data(web::Data::new(meta))`. Without one the route returns `404`.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route(
        "/.well-known/openid-configuration",
        web::get().to(discovery_handler),
    );
}

// ── Metadata builder ──────────────────────────────────────────────────────────

/// Build a [`ProviderMetadata`] document derived from an [`OAuthConfig`].
///
/// Endpoint URLs are constructed as `{issuer}/oauth/{path}`. Override
/// individual fields afterwards for custom deployments (e.g. a different
/// token endpoint path).
///
/// ```no_run
/// use klauthed_web::oauth::{OAuthConfig, discovery};
///
/// # let oauth_config: OAuthConfig = todo!();
/// let mut meta = discovery::build_metadata(&oauth_config);
/// // Override the JWKS URI for an RS256 deployment:
/// meta.jwks_uri = Some("https://auth.example.com/oauth/jwks".into());
/// ```
pub fn build_metadata(config: &OAuthConfig) -> ProviderMetadata {
    let issuer = &config.issuer;

    let mut grant_types = vec![GrantType::AuthorizationCode];
    if config.refresh_token_store.is_some() {
        grant_types.push(GrantType::RefreshToken);
    }

    ProviderMetadata {
        issuer: issuer.clone(),
        authorization_endpoint: Some(format!("{issuer}/oauth/authorize")),
        token_endpoint: Some(format!("{issuer}/oauth/token")),
        // JWKS: left None by default — override when using RS256.
        jwks_uri: None,
        response_types_supported: vec![ResponseType::Code],
        grant_types_supported: grant_types,
        subject_types_supported: vec![SubjectType::Public],
        // HS256 is the default algorithm; override when using RS256.
        id_token_signing_alg_values_supported: vec!["HS256".into()],
        token_endpoint_auth_methods_supported: vec![
            "client_secret_basic".into(),
            "client_secret_post".into(),
            "none".into(),
        ],
        code_challenge_methods_supported: vec!["S256".into(), "plain".into()],
        scopes_supported: vec![
            klauthed_protocol::oidc::Scope::Known(klauthed_protocol::oidc::KnownScope::OpenId),
            klauthed_protocol::oidc::Scope::Known(klauthed_protocol::oidc::KnownScope::Email),
            klauthed_protocol::oidc::Scope::Known(klauthed_protocol::oidc::KnownScope::Profile),
            klauthed_protocol::oidc::Scope::Known(
                klauthed_protocol::oidc::KnownScope::OfflineAccess,
            ),
        ],
        claims_supported: vec![
            "sub".into(),
            "iss".into(),
            "aud".into(),
            "exp".into(),
            "iat".into(),
            "jti".into(),
            "scope".into(),
            "client_id".into(),
        ],
        ..Default::default()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use actix_web::http::StatusCode;
    use actix_web::test as http_test;
    use actix_web::{web, App};
    use klauthed_security::{InMemoryClientStore, InMemoryAuthCodeStore, JwtSigner};

    fn test_config() -> OAuthConfig {
        OAuthConfig::builder()
            .client_store(Arc::new(InMemoryClientStore::new()))
            .code_store(Arc::new(InMemoryAuthCodeStore::new()))
            .signer(JwtSigner::hs256(b"test"))
            .issuer("https://auth.example.com")
            .build()
    }

    #[actix_web::test]
    async fn discovery_returns_provider_metadata() {
        let config = test_config();
        let meta = build_metadata(&config);

        let app = http_test::init_service(
            App::new()
                .app_data(web::Data::new(meta))
                .configure(configure),
        )
        .await;

        let req = http_test::TestRequest::get()
            .uri("/.well-known/openid-configuration")
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body = http_test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["issuer"], "https://auth.example.com");
        assert_eq!(
            json["authorization_endpoint"],
            "https://auth.example.com/oauth/authorize"
        );
        assert_eq!(
            json["token_endpoint"],
            "https://auth.example.com/oauth/token"
        );
        assert_eq!(json["response_types_supported"][0], "code");
        assert!(json["code_challenge_methods_supported"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "S256"));
    }

    #[actix_web::test]
    async fn discovery_404_without_metadata() {
        let app = http_test::init_service(App::new().configure(configure)).await;
        let req = http_test::TestRequest::get()
            .uri("/.well-known/openid-configuration")
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[actix_web::test]
    async fn refresh_grant_appears_when_store_configured() {
        use klauthed_security::InMemoryRefreshTokenStore;
        let config = OAuthConfig::builder()
            .client_store(Arc::new(InMemoryClientStore::new()))
            .code_store(Arc::new(InMemoryAuthCodeStore::new()))
            .refresh_token_store(Arc::new(InMemoryRefreshTokenStore::new()))
            .signer(JwtSigner::hs256(b"test"))
            .issuer("https://auth.example.com")
            .build();

        let meta = build_metadata(&config);
        let grant_types = meta
            .grant_types_supported
            .iter()
            .map(|g| serde_json::to_value(g).unwrap().as_str().unwrap().to_owned())
            .collect::<Vec<_>>();

        assert!(grant_types.contains(&"authorization_code".to_owned()));
        assert!(grant_types.contains(&"refresh_token".to_owned()));
    }
}
