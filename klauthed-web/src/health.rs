//! Liveness and readiness endpoints.
//!
//! Two concerns, kept separate per the usual probe semantics:
//!
//! * **Liveness** (`GET /health`) — always `200`; the process is running.
//! * **Readiness** (`GET /health/ready`) — `200` only when every registered
//!   [`HealthCheck`] reports [`HealthStatus::Up`]; otherwise `503`. The JSON body
//!   summarizes each check so operators can see *what* is unhealthy.
//!
//! Register checks on a [`HealthRegistry`], stash it as app data, and mount the
//! routes with [`configure`]:
//!
//! ```no_run
//! use std::sync::Arc;
//! use actix_web::{web, App};
//! use klauthed_web::health::{HealthCheck, HealthRegistry, HealthStatus};
//!
//! struct Db;
//! #[async_trait::async_trait]
//! impl HealthCheck for Db {
//!     fn name(&self) -> &str { "database" }
//!     async fn check(&self) -> HealthStatus { HealthStatus::Up }
//! }
//!
//! let registry = HealthRegistry::new().with_check(Arc::new(Db));
//! let app = App::new()
//!     .app_data(web::Data::new(registry))
//!     .configure(klauthed_web::health::configure);
//! ```

use std::sync::Arc;

use actix_web::http::StatusCode;
use actix_web::{web, HttpResponse};
use serde::Serialize;

/// Health of a single component or of the service overall.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    /// Fully healthy.
    Up,
    /// Reachable but impaired (e.g. degraded latency, partial capacity).
    Degraded,
    /// Unhealthy / unreachable.
    Down,
}

impl HealthStatus {
    /// Whether this status counts as ready (only [`HealthStatus::Up`] does).
    pub fn is_ready(self) -> bool {
        matches!(self, HealthStatus::Up)
    }

    /// The lowercase wire string (`"up"`, `"degraded"`, `"down"`).
    pub fn as_str(self) -> &'static str {
        match self {
            HealthStatus::Up => "up",
            HealthStatus::Degraded => "degraded",
            HealthStatus::Down => "down",
        }
    }

    /// The worse (less healthy) of two statuses, ordered `Up < Degraded < Down`.
    fn worse(self, other: HealthStatus) -> HealthStatus {
        self.max(other)
    }
}

impl PartialOrd for HealthStatus {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for HealthStatus {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        fn rank(s: HealthStatus) -> u8 {
            match s {
                HealthStatus::Up => 0,
                HealthStatus::Degraded => 1,
                HealthStatus::Down => 2,
            }
        }
        rank(*self).cmp(&rank(*other))
    }
}

/// An asynchronous health probe for one dependency (db, cache, broker, …).
///
/// Implementations should be cheap and time-bounded; readiness aggregates all of
/// them on each request.
#[async_trait::async_trait]
pub trait HealthCheck: Send + Sync + 'static {
    /// A short, stable name identifying this check in the response body.
    fn name(&self) -> &str;

    /// Probe the dependency and report its current [`HealthStatus`].
    async fn check(&self) -> HealthStatus;
}

/// A set of [`HealthCheck`]s aggregated by the readiness handler.
///
/// Cheap to clone (checks are behind `Arc`); register checks before mounting.
#[derive(Clone, Default)]
pub struct HealthRegistry {
    checks: Vec<Arc<dyn HealthCheck>>,
}

impl HealthRegistry {
    /// An empty registry (readiness is trivially ready until checks are added).
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a check in place.
    pub fn register(&mut self, check: Arc<dyn HealthCheck>) {
        self.checks.push(check);
    }

    /// Register a check (builder form).
    pub fn with_check(mut self, check: Arc<dyn HealthCheck>) -> Self {
        self.register(check);
        self
    }

    /// The number of registered checks.
    pub fn len(&self) -> usize {
        self.checks.len()
    }

    /// Whether no checks are registered.
    pub fn is_empty(&self) -> bool {
        self.checks.is_empty()
    }

    /// Run every check and aggregate the results into a [`ReadinessReport`].
    pub async fn report(&self) -> ReadinessReport {
        let mut entries = Vec::with_capacity(self.checks.len());
        let mut overall = HealthStatus::Up;

        for check in &self.checks {
            let status = check.check().await;
            overall = overall.worse(status);
            entries.push(CheckResult {
                name: check.name().to_owned(),
                status,
            });
        }

        ReadinessReport {
            status: overall,
            checks: entries,
        }
    }
}

/// The result of one named check.
#[derive(Debug, Clone, Serialize)]
pub struct CheckResult {
    /// The check's [`HealthCheck::name`].
    pub name: String,
    /// Its reported status.
    pub status: HealthStatus,
}

/// The aggregated outcome of running every registered check.
#[derive(Debug, Clone, Serialize)]
pub struct ReadinessReport {
    /// The overall status: the worst of all checks (`Up` when there are none).
    pub status: HealthStatus,
    /// Per-check results.
    pub checks: Vec<CheckResult>,
}

impl ReadinessReport {
    /// Whether every check is [`HealthStatus::Up`].
    pub fn is_ready(&self) -> bool {
        self.status.is_ready()
    }

    /// The HTTP status the readiness endpoint should return: `200` if ready,
    /// else `503`.
    pub fn http_status(&self) -> StatusCode {
        if self.is_ready() {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        }
    }
}

/// A bare `{ "status": "up" }` body for the liveness probe.
#[derive(Serialize)]
struct LivenessBody {
    status: HealthStatus,
}

/// Liveness handler: the process is up, so this always returns `200`.
async fn liveness() -> HttpResponse {
    HttpResponse::Ok().json(LivenessBody {
        status: HealthStatus::Up,
    })
}

/// Readiness handler: `200` when all checks are `Up`, `503` otherwise.
async fn readiness(registry: Option<web::Data<HealthRegistry>>) -> HttpResponse {
    let report = match registry {
        Some(registry) => registry.report().await,
        // No registry mounted ⇒ nothing to depend on ⇒ ready.
        None => ReadinessReport {
            status: HealthStatus::Up,
            checks: Vec::new(),
        },
    };

    HttpResponse::build(report.http_status()).json(report)
}

/// Mount the health routes (`GET /health`, `GET /health/ready`) on an app or
/// scope. Pass to `App::configure` / `Scope::configure`.
///
/// Add a [`HealthRegistry`] via `app_data(web::Data::new(registry))` to have the
/// readiness probe aggregate real checks; without one it reports ready.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/health", web::get().to(liveness))
        .route("/health/ready", web::get().to(readiness));
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};

    struct Static {
        name: &'static str,
        status: HealthStatus,
    }

    #[async_trait::async_trait]
    impl HealthCheck for Static {
        fn name(&self) -> &str {
            self.name
        }
        async fn check(&self) -> HealthStatus {
            self.status
        }
    }

    fn check(name: &'static str, status: HealthStatus) -> Arc<dyn HealthCheck> {
        Arc::new(Static { name, status })
    }

    #[std::prelude::v1::test]
    fn status_ordering_and_readiness() {
        assert!(HealthStatus::Up.is_ready());
        assert!(!HealthStatus::Degraded.is_ready());
        assert!(!HealthStatus::Down.is_ready());
        assert!(HealthStatus::Up < HealthStatus::Degraded);
        assert!(HealthStatus::Degraded < HealthStatus::Down);
        assert_eq!(HealthStatus::Up.worse(HealthStatus::Down), HealthStatus::Down);
    }

    #[actix_web::test]
    async fn empty_registry_is_ready() {
        let report = HealthRegistry::new().report().await;
        assert!(report.is_ready());
        assert_eq!(report.http_status(), StatusCode::OK);
        assert!(report.checks.is_empty());
    }

    #[actix_web::test]
    async fn report_aggregates_to_worst_status() {
        let registry = HealthRegistry::new()
            .with_check(check("db", HealthStatus::Up))
            .with_check(check("cache", HealthStatus::Degraded));
        let report = registry.report().await;
        assert!(!report.is_ready());
        assert_eq!(report.status, HealthStatus::Degraded);
        assert_eq!(report.checks.len(), 2);
    }

    #[actix_web::test]
    async fn liveness_is_always_ok() {
        let app = test::init_service(App::new().configure(configure)).await;
        let req = test::TestRequest::get().uri("/health").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body = test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "up");
    }

    #[actix_web::test]
    async fn readiness_503_when_a_check_is_down() {
        let registry = HealthRegistry::new()
            .with_check(check("db", HealthStatus::Up))
            .with_check(check("broker", HealthStatus::Down));

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(registry))
                .configure(configure),
        )
        .await;

        let req = test::TestRequest::get().uri("/health/ready").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

        let body = test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "down");
        assert_eq!(json["checks"].as_array().unwrap().len(), 2);
    }

    #[actix_web::test]
    async fn readiness_200_when_all_up() {
        let registry = HealthRegistry::new().with_check(check("db", HealthStatus::Up));
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(registry))
                .configure(configure),
        )
        .await;

        let req = test::TestRequest::get().uri("/health/ready").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[actix_web::test]
    async fn readiness_ready_without_registry() {
        let app = test::init_service(App::new().configure(configure)).await;
        let req = test::TestRequest::get().uri("/health/ready").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
