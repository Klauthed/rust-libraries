//! Public-API integration tests for the rate-limit middleware: budget
//! exhaustion (429 + Retry-After), per-key isolation, and JWT-principal keying.

use std::time::Duration;

use actix_web::http::StatusCode;
use actix_web::http::header::RETRY_AFTER;
use actix_web::{App, HttpResponse, test, web};
use klauthed_web::ratelimit::{KeyBy, RateLimit};

async fn ok() -> HttpResponse {
    HttpResponse::Ok().finish()
}

#[actix_web::test]
async fn middleware_allows_n_then_429_with_retry_after() {
    let limiter = RateLimit::new(2, Duration::from_secs(60)).key_by(KeyBy::header("x-api-key"));
    let app = test::init_service(App::new().wrap(limiter).route("/", web::get().to(ok))).await;

    let make =
        || test::TestRequest::get().uri("/").insert_header(("x-api-key", "client-1")).to_request();

    assert_eq!(test::call_service(&app, make()).await.status(), StatusCode::OK);
    assert_eq!(test::call_service(&app, make()).await.status(), StatusCode::OK);

    let resp = test::call_service(&app, make()).await;
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    let retry =
        resp.headers().get(RETRY_AFTER).expect("Retry-After header present").to_str().unwrap();
    assert!(retry.parse::<u64>().unwrap() >= 1);
}

#[actix_web::test]
async fn distinct_clients_have_separate_budgets() {
    let limiter = RateLimit::new(1, Duration::from_secs(60)).key_by(KeyBy::header("x-api-key"));
    let app = test::init_service(App::new().wrap(limiter).route("/", web::get().to(ok))).await;

    let req_a = test::TestRequest::get().uri("/").insert_header(("x-api-key", "a")).to_request();
    let req_b = test::TestRequest::get().uri("/").insert_header(("x-api-key", "b")).to_request();

    assert_eq!(test::call_service(&app, req_a).await.status(), StatusCode::OK);
    assert_eq!(test::call_service(&app, req_b).await.status(), StatusCode::OK);
}

#[actix_web::test]
async fn principal_key_uses_jwt_sub_when_present() {
    use klauthed_security::{JwtVerifier, jwt::JwtSigner};
    use klauthed_web::auth::JwtAuth;

    const SECRET: &[u8] = b"ratelimit-test-secret";

    // Mint a token for "alice".
    let token = JwtSigner::hs256(SECRET)
        .encode(
            &klauthed_security::Claims::builder(
                "alice",
                &klauthed_core::time::SystemClock,
                klauthed_core::time::Duration::hours(1),
            )
            .build(),
        )
        .unwrap();

    // 1 request allowed per user; alice and bob have independent budgets.
    let limiter = RateLimit::new(1, Duration::from_secs(60)).key_by(KeyBy::Principal);
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(JwtVerifier::hs256(SECRET)))
            .wrap(limiter)
            .wrap(JwtAuth::new())
            .route("/", web::get().to(ok)),
    )
    .await;

    // First request as alice: allowed.
    let req1 = test::TestRequest::get()
        .uri("/")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    assert_eq!(test::call_service(&app, req1).await.status(), StatusCode::OK);

    // Second request as alice: rate-limited.
    let req2 = test::TestRequest::get()
        .uri("/")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    assert_eq!(test::call_service(&app, req2).await.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[actix_web::test]
async fn with_store_injects_a_shared_clock_driven_limiter() {
    use std::sync::Arc;

    use klauthed_core::time::{Duration as CoreDuration, FixedClock};
    use klauthed_web::ratelimit::InMemoryRateLimiter;

    // Inject a clock-driven store so we can deterministically cross the window.
    let clock = Arc::new(FixedClock::at_unix_millis(0));
    let limiter = Arc::new(InMemoryRateLimiter::new(clock.clone()));
    let rl = RateLimit::with_store(limiter, 1, Duration::from_secs(60))
        .key_by(KeyBy::header("x-api-key"));
    let app = test::init_service(App::new().wrap(rl).route("/", web::get().to(ok))).await;

    let make = || test::TestRequest::get().uri("/").insert_header(("x-api-key", "c")).to_request();

    assert_eq!(test::call_service(&app, make()).await.status(), StatusCode::OK);
    assert_eq!(test::call_service(&app, make()).await.status(), StatusCode::TOO_MANY_REQUESTS);

    // Advancing past the window refreshes the budget.
    clock.advance(CoreDuration::seconds(61));
    assert_eq!(test::call_service(&app, make()).await.status(), StatusCode::OK);
}
