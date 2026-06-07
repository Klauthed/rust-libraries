//! Public-API integration test: mount the health endpoints and hit them as a
//! client would.

use actix_web::{App, test, web};
use klauthed_web::health::{HealthRegistry, configure};

#[actix_web::test]
async fn liveness_endpoint_returns_success() {
    let app = test::init_service(
        App::new().app_data(web::Data::new(HealthRegistry::new())).configure(configure),
    )
    .await;

    let req = test::TestRequest::get().uri("/health").to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
}

#[actix_web::test]
async fn readiness_endpoint_ok_with_no_checks() {
    // An empty registry has nothing failing, so readiness is healthy.
    let app = test::init_service(
        App::new().app_data(web::Data::new(HealthRegistry::new())).configure(configure),
    )
    .await;

    let req = test::TestRequest::get().uri("/health/ready").to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
}
