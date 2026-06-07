//! Public-API integration tests for the auth middleware and extractors,
//! exercised together through an actix `App` as a downstream consumer would.

use klauthed_web::auth::{AuthenticatedUser, JwtAuth, OptionalAuthentication, TokenRevocationCheck};
use actix_web::http::StatusCode;
use actix_web::test as http_test;
use actix_web::{App, HttpResponse, web};
use klauthed_core::time::{Duration, SystemClock};
use klauthed_security::JwtVerifier;
use klauthed_security::jwt::{Claims, JwtSigner};

const SECRET: &[u8] = b"test-signing-secret";

fn signer() -> JwtSigner {
    JwtSigner::hs256(SECRET)
}

fn verifier() -> JwtVerifier {
    JwtVerifier::hs256(SECRET)
}

/// Mint a fresh, valid HS256 token for `subject`.
fn valid_token(subject: &str) -> String {
    signer().encode(&Claims::builder(subject, &SystemClock, Duration::hours(1)).build()).unwrap()
}

/// Mint a token whose `exp` is already in the past.
fn expired_token() -> String {
    signer().encode(&Claims::builder("u", &SystemClock, Duration::hours(-1)).build()).unwrap()
}

async fn echo_sub(user: AuthenticatedUser) -> HttpResponse {
    HttpResponse::Ok().body(user.sub().unwrap_or("").to_owned())
}

async fn echo_optional(auth: OptionalAuthentication) -> HttpResponse {
    match auth.into_inner() {
        Some(c) => HttpResponse::Ok().body(c.sub.unwrap_or_default()),
        None => HttpResponse::Ok().body("anonymous"),
    }
}

macro_rules! auth_app {
    () => {
        http_test::init_service(
            App::new()
                .app_data(web::Data::new(verifier()))
                .wrap(JwtAuth::new())
                .route("/protected", web::get().to(echo_sub))
                .route("/optional", web::get().to(echo_optional)),
        )
        .await
    };
}

// Handler that reads RequestContext.principal() to verify propagation.
async fn echo_principal(ctx: klauthed_web::context::Context) -> HttpResponse {
    HttpResponse::Ok().body(ctx.principal().unwrap_or("none").to_owned())
}

#[actix_web::test]
async fn jwt_auth_propagates_sub_into_request_context() {
    let app = http_test::init_service(
        App::new()
            .app_data(web::Data::new(verifier()))
            .wrap(JwtAuth::new())
            .wrap(klauthed_web::context::RequestContextMiddleware::new())
            .route("/principal", web::get().to(echo_principal)),
    )
    .await;

    let token = valid_token("alice");
    let req = http_test::TestRequest::get()
        .uri("/principal")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = http_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // RequestContext.principal() must equal the JWT sub claim.
    let body = http_test::read_body(resp).await;
    assert_eq!(&body[..], b"alice");
}

#[actix_web::test]
async fn valid_token_reaches_handler_with_claims() {
    let app = auth_app!();
    let token = valid_token("alice");
    let req = http_test::TestRequest::get()
        .uri("/protected")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let resp = http_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = http_test::read_body(resp).await;
    assert_eq!(&body[..], b"alice");
}

#[actix_web::test]
async fn missing_authorization_header_returns_401() {
    let app = auth_app!();
    let req = http_test::TestRequest::get().uri("/protected").to_request();
    let resp = http_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn non_bearer_scheme_returns_401() {
    let app = auth_app!();
    let req = http_test::TestRequest::get()
        .uri("/protected")
        .insert_header(("Authorization", "Basic dXNlcjpwYXNz"))
        .to_request();
    let resp = http_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn expired_token_returns_401() {
    let app = auth_app!();
    let token = expired_token();
    let req = http_test::TestRequest::get()
        .uri("/protected")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let resp = http_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let body = http_test::read_body(resp).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"]["code"], "security.expired_token");
}

#[actix_web::test]
async fn wrong_secret_returns_401() {
    let app = auth_app!();
    let token = JwtSigner::hs256(b"wrong-secret")
        .encode(&Claims::builder("eve", &SystemClock, Duration::hours(1)).build())
        .unwrap();
    let req = http_test::TestRequest::get()
        .uri("/protected")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let resp = http_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let body = http_test::read_body(resp).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"]["code"], "security.invalid_token");
}

#[actix_web::test]
async fn malformed_token_returns_400() {
    let app = auth_app!();
    let req = http_test::TestRequest::get()
        .uri("/protected")
        .insert_header(("Authorization", "Bearer not.a.jwt"))
        .to_request();
    let resp = http_test::call_service(&app, req).await;
    // MalformedToken → BadRequest (400).
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn optional_authentication_no_token_rejected_by_middleware() {
    // JwtAuth wraps the whole app, so a missing token is rejected before
    // the OptionalAuthentication extractor even runs.
    let app = auth_app!();
    let req = http_test::TestRequest::get().uri("/optional").to_request();
    let resp = http_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

/// Test the OptionalAuthentication extractor in isolation (without JwtAuth).
#[actix_web::test]
async fn optional_extractor_without_middleware_returns_none() {
    let app =
        http_test::init_service(App::new().route("/optional", web::get().to(echo_optional))).await;

    let req = http_test::TestRequest::get().uri("/optional").to_request();
    let resp = http_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body = http_test::read_body(resp).await;
    assert_eq!(&body[..], b"anonymous");
}

/// Test the OptionalAuthentication extractor when JwtAuth ran successfully.
#[actix_web::test]
async fn optional_extractor_with_middleware_returns_some() {
    let app = http_test::init_service(
        App::new()
            .app_data(web::Data::new(verifier()))
            .wrap(JwtAuth::new())
            .route("/optional", web::get().to(echo_optional)),
    )
    .await;

    let token = valid_token("bob");
    let req = http_test::TestRequest::get()
        .uri("/optional")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();

    let resp = http_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body = http_test::read_body(resp).await;
    assert_eq!(&body[..], b"bob");
}

#[actix_web::test]
async fn missing_verifier_returns_500() {
    // No web::Data<JwtVerifier> registered → configuration error → 500.
    let app = http_test::init_service(
        App::new()
            .wrap(JwtAuth::new())
            .route("/", web::get().to(|| async { HttpResponse::Ok().finish() })),
    )
    .await;

    let req = http_test::TestRequest::get()
        .uri("/")
        .insert_header(("Authorization", format!("Bearer {}", valid_token("u"))))
        .to_request();

    let resp = http_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[actix_web::test]
async fn authenticated_user_extractor_fails_without_claims_in_extensions() {
    // No JwtAuth middleware → no claims in extensions → extractor returns Err.
    let app = http_test::init_service(App::new().route("/", web::get().to(echo_sub))).await;

    let req = http_test::TestRequest::get().uri("/").to_request();
    let resp = http_test::call_service(&app, req).await;
    // Handler requires AuthenticatedUser; no claims → 401.
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn revoked_token_jti_returns_401() {
    use klauthed_security::{InMemoryTokenDenylist, TokenDenylist as _};
    use std::sync::Arc;

    let denylist = Arc::new(InMemoryTokenDenylist::new());

    let jti = "unique-jti-abc123";
    let token = JwtSigner::hs256(SECRET)
        .encode(&Claims::builder("alice", &SystemClock, Duration::hours(1)).jwt_id(jti).build())
        .unwrap();

    // A concrete far-future expiry (~10 years out) so the denylist entry is
    // not evicted during the test.
    let far_future =
        klauthed_core::time::Timestamp::now().checked_add(Duration::days(365 * 10)).unwrap();
    denylist.revoke(jti.into(), far_future).await.unwrap();

    let app = http_test::init_service(
        App::new()
            .app_data(web::Data::new(verifier()))
            .app_data(web::Data::new(TokenRevocationCheck(
                denylist as Arc<dyn klauthed_security::TokenDenylist>,
            )))
            .wrap(JwtAuth::new())
            .route("/", web::get().to(echo_sub)),
    )
    .await;

    let req = http_test::TestRequest::get()
        .uri("/")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let resp = http_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn non_revoked_token_passes_denylist_check() {
    use klauthed_security::InMemoryTokenDenylist;
    use std::sync::Arc;

    let token = JwtSigner::hs256(SECRET)
        .encode(
            &Claims::builder("alice", &SystemClock, Duration::hours(1))
                .jwt_id("not-revoked")
                .build(),
        )
        .unwrap();

    let app = http_test::init_service(
        App::new()
            .app_data(web::Data::new(verifier()))
            .app_data(web::Data::new(TokenRevocationCheck(Arc::new(InMemoryTokenDenylist::new()))))
            .wrap(JwtAuth::new())
            .route("/", web::get().to(echo_sub)),
    )
    .await;

    let req = http_test::TestRequest::get()
        .uri("/")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let resp = http_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
}
