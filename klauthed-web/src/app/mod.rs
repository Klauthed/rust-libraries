//! Auto-wired application components.
//!
//! [`Components`] is the Spring Boot Actuator equivalent for klauthed services:
//! each infrastructure dependency you add (database pool, Redis connection, …)
//! **automatically** contributes a health check to the readiness probe and
//! registers itself as [`web::Data`] for use in handlers.
//!
//! # Minimal example
//!
//! No `HealthRegistry`, no `SqlHealthCheck`, no `app_data` calls — just declare
//! what you have:
//!
//! ```no_run
//! use klauthed_web::{app::Components, server};
//! use klauthed_core::config::ServerConfig;
//!
//! # async fn run() -> std::io::Result<()> {
//! let config = ServerConfig::default();
//!
//! // (with feature = "data-sql" enabled and a real pool)
//! // let pool = sqlx::AnyPool::connect(&url).await?;
//!
//! let components = Components::new();
//! // .pool("database", pool)  ← auto-adds SqlHealthCheck + web::Data<AnyPool>
//!
//! server::serve_with_components(&config, components, |cfg| {
//!     // Only your own routes go here — health is wired automatically.
//!     // cfg.route("/api/users", web::get().to(list_users));
//! })?
//! .await
//! # }
//! ```
//!
//! `/health` is always `200` (liveness). `/health/ready` returns `200` when
//! every auto-registered check is `Up`, `503` otherwise — with a JSON body
//! identifying which component is failing.
//!
//! # Custom checks
//!
//! Add any [`HealthCheck`] alongside the
//! auto-detected ones:
//!
//! ```no_run
//! use std::sync::Arc;
//! use klauthed_web::app::Components;
//! use klauthed_web::health::{HealthCheck, HealthStatus};
//!
//! struct ThirdPartyApi;
//! #[async_trait::async_trait]
//! impl HealthCheck for ThirdPartyApi {
//!     fn name(&self) -> &str { "payments-api" }
//!     async fn check(&self) -> HealthStatus { HealthStatus::Up }
//! }
//!
//! let components = Components::new()
//!     .check(Arc::new(ThirdPartyApi));
//! ```

use std::sync::Arc;

use actix_web::web;

use crate::health::{HealthCheck, HealthRegistry};

/// A closure that registers `web::Data` entries on a [`web::ServiceConfig`].
///
/// `Arc<dyn Fn>` makes the holding vec `Clone + Send + Sync` so [`Components`]
/// can be cloned per actix worker.
type DataFn = Arc<dyn Fn(&mut web::ServiceConfig) + Send + Sync + 'static>;

// ── Components ────────────────────────────────────────────────────────────────

/// A collected set of application infrastructure components.
///
/// Each method adds a component that:
/// 1. Gets registered as [`web::Data<T>`] so handlers can extract it.
/// 2. Contributes an appropriate [`HealthCheck`] to the readiness probe
///    automatically.
///
/// `Components` is cheap to clone (all internals are reference-counted). Build
/// it once outside the actix worker factory and let `serve_with_components`
/// clone it per worker.
#[derive(Clone)]
pub struct Components {
    registry: HealthRegistry,
    /// Closures that add `web::Data` entries to a [`web::ServiceConfig`].
    data_fns: Vec<DataFn>,
}

impl Default for Components {
    fn default() -> Self {
        Self::new()
    }
}

impl Components {
    /// Create an empty set of components. No checks are registered until you
    /// add infra via the builder methods.
    #[must_use]
    pub fn new() -> Self {
        Self { registry: HealthRegistry::new(), data_fns: Vec::new() }
    }

    /// Register a custom [`HealthCheck`] without contributing app data.
    ///
    /// Use this for checks that don't map to a concrete infrastructure type —
    /// for example, a third-party API ping or a business-logic invariant.
    #[must_use]
    pub fn check(mut self, check: Arc<dyn HealthCheck>) -> Self {
        self.registry.register(check);
        self
    }

    // ── SQL pool ──────────────────────────────────────────────────────────────

    /// Add a `sqlx::AnyPool`, registering it as `web::Data<AnyPool>` and
    /// adding an [`SqlHealthCheck`](crate::health::SqlHealthCheck) that probes
    /// the pool with `SELECT 1`.
    ///
    /// `name` appears in the readiness report so operators know which database
    /// is unhealthy (e.g. `"primary"`, `"read-replica"`).
    ///
    /// Requires feature `data-sql`.
    #[cfg(feature = "data-sql")]
    #[must_use]
    pub fn pool(mut self, name: impl Into<String>, pool: sqlx::AnyPool) -> Self {
        use crate::health::SqlHealthCheck;
        self.registry.register(Arc::new(SqlHealthCheck::new(name, pool.clone())));
        let p = pool.clone();
        self.data_fns.push(Arc::new(move |cfg: &mut web::ServiceConfig| {
            cfg.app_data(web::Data::new(p.clone()));
        }));
        self
    }

    // ── Redis connection ──────────────────────────────────────────────────────

    /// Add a `redis::aio::ConnectionManager`, registering it as
    /// `web::Data<ConnectionManager>` and adding a
    /// [`RedisHealthCheck`](crate::health::RedisHealthCheck) that sends `PING`.
    ///
    /// Requires feature `data-redis`.
    #[cfg(feature = "data-redis")]
    #[must_use]
    pub fn redis(mut self, name: impl Into<String>, conn: redis::aio::ConnectionManager) -> Self {
        use crate::health::RedisHealthCheck;
        self.registry.register(Arc::new(RedisHealthCheck::new(name, conn.clone())));
        let c = conn.clone();
        self.data_fns.push(Arc::new(move |cfg: &mut web::ServiceConfig| {
            cfg.app_data(web::Data::new(c.clone()));
        }));
        self
    }

    // ── Internal ──────────────────────────────────────────────────────────────

    /// Number of health checks currently registered.
    #[must_use]
    pub fn check_count(&self) -> usize {
        self.registry.len()
    }

    /// Read access to the underlying [`HealthRegistry`] (for testing).
    #[must_use]
    pub fn registry(&self) -> &HealthRegistry {
        &self.registry
    }

    /// Apply all components to a [`web::ServiceConfig`].
    ///
    /// Called once per actix worker inside the app factory. Adds
    /// `web::Data<HealthRegistry>` (so the readiness handler finds it) plus
    /// every infra item registered via the builder methods.
    pub(crate) fn configure(&self, cfg: &mut web::ServiceConfig) {
        // The readiness handler uses Option<web::Data<HealthRegistry>>; we
        // always register it so the auto-detected checks are included.
        cfg.app_data(web::Data::new(self.registry.clone()));
        for f in &self.data_fns {
            f(cfg);
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::{HealthCheck, HealthStatus};

    struct AlwaysUp;

    #[async_trait::async_trait]
    impl HealthCheck for AlwaysUp {
        fn name(&self) -> &str {
            "always-up"
        }
        async fn check(&self) -> HealthStatus {
            HealthStatus::Up
        }
    }

    #[test]
    fn empty_components_has_no_checks() {
        let c = Components::new();
        assert_eq!(c.check_count(), 0);
    }

    #[test]
    fn custom_check_is_registered() {
        let c = Components::new().check(Arc::new(AlwaysUp));
        assert_eq!(c.check_count(), 1);
    }

    #[actix_web::test]
    async fn registry_from_components_reports_up() {
        let c = Components::new().check(Arc::new(AlwaysUp));
        let report = c.registry().report().await;
        assert!(report.is_ready());
        assert_eq!(report.checks[0].name, "always-up");
    }

    #[test]
    fn components_is_clone() {
        let a = Components::new().check(Arc::new(AlwaysUp));
        let b = a.clone();
        assert_eq!(b.check_count(), 1);
    }

    #[actix_web::test]
    async fn configure_registers_health_registry_as_app_data() {
        use actix_web::App;
        use actix_web::test as http_test;

        let components = Components::new().check(Arc::new(AlwaysUp));

        // Verify the registry lands in app_data by querying /health/ready.
        let app = http_test::init_service(
            App::new()
                .configure(|cfg| components.configure(cfg))
                .configure(crate::health::configure),
        )
        .await;

        let req = http_test::TestRequest::get().uri("/health/ready").to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);

        let body = http_test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "up");
        assert_eq!(json["checks"][0]["name"], "always-up");
    }
}
