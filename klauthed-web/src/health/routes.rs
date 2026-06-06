//! HTTP handlers for liveness and readiness probes.
//!
//! * `GET /health`       — liveness (always `200`).
//! * `GET /health/ready` — readiness (`200` / `503`).
//!
//! Mount with [`configure`].

use actix_web::{web, HttpResponse};
use serde::Serialize;

use super::registry::{HealthRegistry, ReadinessReport};
use super::status::HealthStatus;

// ── Private response types ────────────────────────────────────────────────────

#[derive(Serialize)]
struct LivenessBody {
    status: HealthStatus,
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// Liveness handler — the process is running, so this always returns `200`.
async fn liveness() -> HttpResponse {
    HttpResponse::Ok().json(LivenessBody {
        status: HealthStatus::Up,
    })
}

/// Readiness handler — `200` when all checks are `Up`, `503` otherwise.
async fn readiness(registry: Option<web::Data<HealthRegistry>>) -> HttpResponse {
    let report = match registry {
        Some(r) => r.report().await,
        // No registry mounted ⇒ nothing to depend on ⇒ trivially ready.
        None => ReadinessReport {
            status: HealthStatus::Up,
            checks: Vec::new(),
        },
    };
    HttpResponse::build(report.http_status()).json(report)
}

// ── Route configuration ───────────────────────────────────────────────────────

/// Mount the health routes on an app or scope.
///
/// Routes added:
/// * `GET /health`       — liveness probe
/// * `GET /health/ready` — readiness probe
///
/// Register a [`HealthRegistry`] via `app_data(web::Data::new(registry))` to
/// surface real checks; without one the readiness probe reports ready.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/health", web::get().to(liveness))
        .route("/health/ready", web::get().to(readiness));
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use actix_web::http::StatusCode;
    use actix_web::test as http_test;
    use actix_web::App;

    use super::super::registry::{HealthCheck, HealthRegistry};

    struct Static(&'static str, HealthStatus);

    #[async_trait::async_trait]
    impl HealthCheck for Static {
        fn name(&self) -> &str { self.0 }
        async fn check(&self) -> HealthStatus { self.1 }
    }

    fn check(name: &'static str, status: HealthStatus) -> Arc<dyn HealthCheck> {
        Arc::new(Static(name, status))
    }

    #[actix_web::test]
    async fn liveness_always_returns_200() {
        let app = http_test::init_service(App::new().configure(configure)).await;
        let req = http_test::TestRequest::get().uri("/health").to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body = http_test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "up");
    }

    #[actix_web::test]
    async fn readiness_200_when_all_up() {
        let registry = HealthRegistry::new().with_check(check("db", HealthStatus::Up));
        let app = http_test::init_service(
            App::new()
                .app_data(web::Data::new(registry))
                .configure(configure),
        )
        .await;

        let req = http_test::TestRequest::get().uri("/health/ready").to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[actix_web::test]
    async fn readiness_503_when_a_check_is_down() {
        let registry = HealthRegistry::new()
            .with_check(check("db", HealthStatus::Up))
            .with_check(check("broker", HealthStatus::Down));

        let app = http_test::init_service(
            App::new()
                .app_data(web::Data::new(registry))
                .configure(configure),
        )
        .await;

        let req = http_test::TestRequest::get().uri("/health/ready").to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

        let body = http_test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "down");
        assert_eq!(json["checks"].as_array().unwrap().len(), 2);
    }

    #[actix_web::test]
    async fn readiness_ready_without_registry() {
        let app = http_test::init_service(App::new().configure(configure)).await;
        let req = http_test::TestRequest::get().uri("/health/ready").to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
