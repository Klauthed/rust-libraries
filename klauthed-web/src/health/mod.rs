//! Liveness and readiness endpoints.
//!
//! Two probe endpoints with separate concerns:
//!
//! * **Liveness** (`GET /health`) — always `200`; the process is alive.
//! * **Readiness** (`GET /health/ready`) — `200` only when every registered
//!   [`HealthCheck`] reports [`HealthStatus::Up`]; otherwise `503`.
//!
//! # Structure
//!
//! | Sub-module | Responsibility |
//! |---|---|
//! | [`status`] | [`HealthStatus`] value type with ordering |
//! | [`registry`] | [`HealthCheck`] trait, [`HealthRegistry`], report types |
//! | [`routes`] | HTTP handlers and [`configure`] |
//! | [`checks`] | Concrete implementations (`SqlHealthCheck`, `RedisHealthCheck`) |
//!
//! # Quick start
//!
//! ```no_run
//! use std::sync::Arc;
//! use actix_web::{web, App};
//! use klauthed_web::health::{configure, HealthCheck, HealthRegistry, HealthStatus};
//!
//! struct MyDb;
//! #[async_trait::async_trait]
//! impl HealthCheck for MyDb {
//!     fn name(&self) -> &str { "database" }
//!     async fn check(&self) -> HealthStatus { HealthStatus::Up }
//! }
//!
//! let registry = HealthRegistry::new().with_check(Arc::new(MyDb));
//! let _app = App::new()
//!     .app_data(web::Data::new(registry))
//!     .configure(configure);
//! ```

pub mod checks;
pub mod registry;
pub mod routes;
pub mod status;

#[cfg(feature = "data-sql")]
pub use checks::SqlHealthCheck;
#[cfg(feature = "data-redis")]
pub use checks::RedisHealthCheck;
pub use registry::{CheckResult, HealthCheck, HealthRegistry, ReadinessReport};
pub use routes::configure;
pub use status::HealthStatus;
