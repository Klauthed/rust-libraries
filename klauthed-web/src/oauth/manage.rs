//! Token revocation (RFC 7009) and introspection (RFC 7662) endpoints.
//!
//! Both require client authentication (same as the token endpoint). They use
//! the optional `verifier` / `token_denylist` on [`OAuthConfig`]:
//!
//! * **Revocation** — a refresh token is consumed and its rotation family
//!   revoked; an access token's `jti` is added to the denylist (which the
//!   resource server's `TokenRevocationCheck` then rejects). Per RFC 7009 the
//!   endpoint always returns `200`, even for an unknown token.
//! * **Introspection** — decodes and validates the access token, returning its
//!   metadata with `active: true`, or `{"active": false}` for any
//!   invalid/expired/revoked/unconfigured case.
//!
//! Mount with [`configure`]; register the same [`OAuthConfig`] as the token
//! endpoint (with `verifier`/`token_denylist` set to enable access-token
//! handling).

use actix_web::{web, HttpResponse};
use klauthed_core::time::Timestamp;
use klauthed_protocol::oauth2::{
    IntrospectionRequest, IntrospectionResponse, OAuth2ErrorCode, RevocationRequest, TokenType,
    TokenTypeHint,
};
use klauthed_security::refresh_token::ConsumeResult;

use super::config::OAuthConfig;
use super::handlers::authenticate_client;
use super::util::token_error;

/// `POST /oauth/revoke` (`application/x-www-form-urlencoded`, RFC 7009).
pub async fn revoke(
    form: web::Form<RevocationRequest>,
    config: web::Data<OAuthConfig>,
) -> HttpResponse {
    let req = form.into_inner();

    // Client authentication is required (RFC 7009 §2.1).
    let client_id = match req.client_id.as_deref() {
        Some(id) => id,
        None => return token_error(OAuth2ErrorCode::InvalidRequest, "client_id is required"),
    };
    if let Err(resp) = authenticate_client(client_id, req.client_secret.as_deref(), &config).await {
        return resp;
    }

    revoke_token(&req.token, req.token_type_hint, &config).await;

    // RFC 7009 §2.2: respond 200 regardless of whether the token was found, so
    // the endpoint does not leak token validity.
    HttpResponse::Ok().finish()
}

/// `POST /oauth/introspect` (`application/x-www-form-urlencoded`, RFC 7662).
pub async fn introspect(
    form: web::Form<IntrospectionRequest>,
    config: web::Data<OAuthConfig>,
) -> HttpResponse {
    let req = form.into_inner();

    // Introspection is a protected resource — authenticate the caller (RFC 7662 §2.1).
    let client_id = match req.client_id.as_deref() {
        Some(id) => id,
        None => return token_error(OAuth2ErrorCode::InvalidRequest, "client_id is required"),
    };
    if let Err(resp) = authenticate_client(client_id, req.client_secret.as_deref(), &config).await {
        return resp;
    }

    HttpResponse::Ok().json(introspect_token(&req.token, &config).await)
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Best-effort revocation of `token`. The `hint` narrows which token type to
/// try; without it, both are attempted. Failures are swallowed (RFC 7009 §2.2).
async fn revoke_token(token: &str, hint: Option<TokenTypeHint>, config: &OAuthConfig) {
    // Refresh token: consume it (removing it from the active set) and revoke
    // its whole rotation family so any sibling tokens are invalidated too.
    if !matches!(hint, Some(TokenTypeHint::AccessToken))
        && let Some(store) = &config.refresh_token_store
        && let Ok(ConsumeResult::Valid(rt) | ConsumeResult::Expired(rt)) = store.consume(token).await
    {
        let _ = store.revoke_family(&rt.family_id).await;
        return;
    }

    // Access token: decode to its `jti`/`exp` and add it to the denylist.
    if !matches!(hint, Some(TokenTypeHint::RefreshToken))
        && let (Some(verifier), Some(denylist)) = (&config.verifier, &config.token_denylist)
        && let Ok(claims) = verifier.decode(token)
        && let (Some(jti), Some(exp)) = (claims.jti, claims.exp)
    {
        let _ = denylist.revoke(jti, Timestamp::from_unix_seconds(exp)).await;
    }
}

/// Introspect an access token, returning its metadata or the inactive response.
async fn introspect_token(token: &str, config: &OAuthConfig) -> IntrospectionResponse {
    // Without a verifier we cannot inspect access tokens.
    let Some(verifier) = &config.verifier else {
        return IntrospectionResponse::inactive();
    };
    // `decode` validates the signature and `exp`/`nbf`; any failure is inactive.
    let claims = match verifier.decode(token) {
        Ok(c) => c,
        Err(_) => return IntrospectionResponse::inactive(),
    };

    // A revoked (denylisted) access token is no longer active.
    if let (Some(denylist), Some(jti)) = (&config.token_denylist, claims.jti.as_deref())
        && denylist.is_revoked(jti).await.unwrap_or(false)
    {
        return IntrospectionResponse::inactive();
    }

    IntrospectionResponse {
        active: true,
        scope: claims
            .custom
            .get("scope")
            .and_then(|v| v.as_str())
            .map(str::to_owned),
        client_id: claims
            .custom
            .get("client_id")
            .and_then(|v| v.as_str())
            .map(str::to_owned),
        sub: claims.sub,
        token_type: Some(TokenType::default()),
        exp: claims.exp,
        iat: claims.iat,
        iss: claims.iss,
        jti: claims.jti,
    }
}

/// Mount the revocation and introspection routes on an app or scope.
///
/// Routes added: `POST /oauth/revoke` and `POST /oauth/introspect`.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/oauth/revoke", web::post().to(revoke))
        .route("/oauth/introspect", web::post().to(introspect));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use actix_web::http::StatusCode;
    use actix_web::test as http_test;
    use actix_web::{web, App};
    use klauthed_core::time::{Clock, Duration, FixedClock, SystemClock, Timestamp};
    use klauthed_security::jwt::{Claims, JwtSigner};
    use klauthed_security::oauth2_client::{
        ClientGrantType, ClientType, InMemoryClientStore, OAuth2Client, TokenEndpointAuthMethod,
    };
    use klauthed_security::refresh_token::{InMemoryRefreshTokenStore, RefreshTokenBuilder};
    use klauthed_security::{InMemoryAuthCodeStore, InMemoryTokenDenylist, JwtVerifier, TokenDenylist};

    const SECRET: &[u8] = b"manage-test-secret";

    fn public_client(id: &str) -> OAuth2Client {
        OAuth2Client {
            client_id: id.into(),
            client_type: ClientType::Public,
            client_secret_hash: None,
            redirect_uris: vec!["https://app.example.com/cb".into()],
            allowed_grant_types: [ClientGrantType::AuthorizationCode, ClientGrantType::RefreshToken]
                .into_iter()
                .collect(),
            allowed_scopes: ["openid", "email"].iter().map(|s| s.to_string()).collect(),
            token_endpoint_auth_method: TokenEndpointAuthMethod::None,
            client_name: None,
            created_at: Timestamp::now(),
        }
    }

    fn make_config(
        clock: Arc<dyn Clock>,
        denylist: Option<Arc<dyn TokenDenylist>>,
        with_refresh: bool,
    ) -> OAuthConfig {
        let mut b = OAuthConfig::builder()
            .client_store(Arc::new(InMemoryClientStore::new()))
            .code_store(Arc::new(InMemoryAuthCodeStore::with_clock(clock.clone())))
            .signer(JwtSigner::hs256(SECRET))
            .verifier(JwtVerifier::hs256(SECRET))
            .issuer("https://test.example.com")
            .clock(clock.clone());
        if with_refresh {
            b = b.refresh_token_store(Arc::new(InMemoryRefreshTokenStore::with_clock(clock)));
        }
        if let Some(d) = denylist {
            b = b.token_denylist(d);
        }
        b.build()
    }

    /// Mint an access token the way the token endpoint does.
    fn access_token(sub: &str, scope: &str, client_id: &str) -> String {
        JwtSigner::hs256(SECRET)
            .encode(
                &Claims::builder(sub, &SystemClock, Duration::hours(1))
                    .issuer("https://test.example.com")
                    .claim("scope", scope)
                    .claim("client_id", client_id)
                    .random_jwt_id()
                    .unwrap()
                    .build(),
            )
            .unwrap()
    }

    macro_rules! app_with {
        ($config:expr) => {
            http_test::init_service(
                App::new()
                    .app_data(web::Data::new($config))
                    .configure(configure),
            )
            .await
        };
    }

    #[actix_web::test]
    async fn introspect_active_access_token() {
        let clock = Arc::new(FixedClock::new(Timestamp::now()));
        let config = make_config(clock, None, false);
        config.client_store.register(public_client("c1")).await.unwrap();
        let token = access_token("alice", "openid email", "c1");

        let app = app_with!(config);
        let form = format!("token={token}&client_id=c1");
        let req = http_test::TestRequest::post()
            .uri("/oauth/introspect")
            .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
            .set_payload(form)
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body = http_test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["active"], true);
        assert_eq!(json["sub"], "alice");
        assert_eq!(json["scope"], "openid email");
        assert_eq!(json["client_id"], "c1");
        assert_eq!(json["token_type"], "Bearer");
    }

    #[actix_web::test]
    async fn introspect_invalid_token_is_inactive() {
        let clock = Arc::new(FixedClock::new(Timestamp::now()));
        let config = make_config(clock, None, false);
        config.client_store.register(public_client("c1")).await.unwrap();

        let app = app_with!(config);
        let req = http_test::TestRequest::post()
            .uri("/oauth/introspect")
            .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
            .set_payload("token=not-a-jwt&client_id=c1")
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body = http_test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["active"], false);
        assert!(json.get("sub").is_none());
    }

    #[actix_web::test]
    async fn introspect_requires_client_id() {
        let clock = Arc::new(FixedClock::new(Timestamp::now()));
        let config = make_config(clock, None, false);

        let app = app_with!(config);
        let req = http_test::TestRequest::post()
            .uri("/oauth/introspect")
            .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
            .set_payload("token=abc")
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[actix_web::test]
    async fn revoke_access_token_then_introspect_inactive() {
        let clock = Arc::new(FixedClock::new(Timestamp::now()));
        let denylist: Arc<dyn TokenDenylist> = Arc::new(InMemoryTokenDenylist::new());
        let config = make_config(clock, Some(Arc::clone(&denylist)), false);
        config.client_store.register(public_client("c1")).await.unwrap();
        let token = access_token("alice", "openid", "c1");

        let app = app_with!(config);

        // Revoke → 200.
        let req = http_test::TestRequest::post()
            .uri("/oauth/revoke")
            .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
            .set_payload(format!("token={token}&client_id=c1&token_type_hint=access_token"))
            .to_request();
        assert_eq!(http_test::call_service(&app, req).await.status(), StatusCode::OK);

        // Introspect the same token → now inactive (denylisted).
        let req = http_test::TestRequest::post()
            .uri("/oauth/introspect")
            .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
            .set_payload(format!("token={token}&client_id=c1"))
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        let body = http_test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["active"], false);
    }

    #[actix_web::test]
    async fn revoke_refresh_token_returns_200_and_invalidates_it() {
        let clock = Arc::new(FixedClock::new(Timestamp::now()));
        let config = make_config(clock.clone(), None, true);
        config.client_store.register(public_client("c1")).await.unwrap();

        let rt = RefreshTokenBuilder::new("c1", "alice")
            .scope(vec!["openid".into()])
            .build(&*clock, Duration::days(30))
            .unwrap();
        let rt_str = rt.token.clone();
        let store = config.refresh_token_store.clone().unwrap();
        store.store(rt).await.unwrap();

        let app = app_with!(config);
        let req = http_test::TestRequest::post()
            .uri("/oauth/revoke")
            .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
            .set_payload(format!("token={rt_str}&client_id=c1&token_type_hint=refresh_token"))
            .to_request();
        assert_eq!(http_test::call_service(&app, req).await.status(), StatusCode::OK);

        // The refresh token is no longer usable.
        assert!(!matches!(
            store.consume(&rt_str).await.unwrap(),
            ConsumeResult::Valid(_)
        ));
    }

    #[actix_web::test]
    async fn revoke_unknown_token_still_returns_200() {
        let clock = Arc::new(FixedClock::new(Timestamp::now()));
        let config = make_config(clock, None, true);
        config.client_store.register(public_client("c1")).await.unwrap();

        let app = app_with!(config);
        let req = http_test::TestRequest::post()
            .uri("/oauth/revoke")
            .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
            .set_payload("token=never-existed&client_id=c1")
            .to_request();
        assert_eq!(http_test::call_service(&app, req).await.status(), StatusCode::OK);
    }
}
