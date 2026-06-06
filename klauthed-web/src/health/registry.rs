//! `HealthCheck` trait, `HealthRegistry`, and the report types.

use std::sync::Arc;

use actix_web::http::StatusCode;
use serde::Serialize;

use super::status::HealthStatus;

// ── HealthCheck ───────────────────────────────────────────────────────────────

/// An asynchronous health probe for one dependency (db, cache, broker, …).
///
/// Implementations should be cheap and time-bounded; the readiness endpoint
/// runs all registered checks on every request.
#[async_trait::async_trait]
pub trait HealthCheck: Send + Sync + 'static {
    /// A short, stable name identifying this check in the response body.
    fn name(&self) -> &str;

    /// Probe the dependency and report its current [`HealthStatus`].
    async fn check(&self) -> HealthStatus;
}

// ── Report types ──────────────────────────────────────────────────────────────

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
    /// The overall status — the worst of all checks (`Up` when there are none).
    pub status: HealthStatus,
    /// Per-check results.
    pub checks: Vec<CheckResult>,
}

impl ReadinessReport {
    /// Whether every check is [`HealthStatus::Up`].
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.status.is_ready()
    }

    /// The HTTP status the readiness endpoint should return: `200` if ready,
    /// `503` otherwise.
    #[must_use]
    pub fn http_status(&self) -> StatusCode {
        if self.is_ready() { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE }
    }
}

// ── HealthRegistry ────────────────────────────────────────────────────────────

/// A set of [`HealthCheck`]s aggregated by the readiness handler.
///
/// Cheap to clone — checks are behind `Arc`. Register checks before mounting
/// the application.
#[derive(Clone, Default)]
pub struct HealthRegistry {
    checks: Vec<Arc<dyn HealthCheck>>,
}

impl HealthRegistry {
    /// An empty registry (readiness is trivially `Up` until checks are added).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a check in place.
    pub fn register(&mut self, check: Arc<dyn HealthCheck>) {
        self.checks.push(check);
    }

    /// Register a check (builder / chaining form).
    #[must_use]
    pub fn with_check(mut self, check: Arc<dyn HealthCheck>) -> Self {
        self.register(check);
        self
    }

    /// Number of registered checks.
    #[must_use]
    pub fn len(&self) -> usize {
        self.checks.len()
    }

    /// Whether no checks are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.checks.is_empty()
    }

    /// Run every check concurrently and aggregate the results into a
    /// [`ReadinessReport`].
    pub async fn report(&self) -> ReadinessReport {
        let mut entries = Vec::with_capacity(self.checks.len());
        let mut overall = HealthStatus::Up;

        for check in &self.checks {
            let status = check.check().await;
            overall = overall.worse(status);
            entries.push(CheckResult { name: check.name().to_owned(), status });
        }

        ReadinessReport { status: overall, checks: entries }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::http::StatusCode;

    struct Static(&'static str, HealthStatus);

    #[async_trait::async_trait]
    impl HealthCheck for Static {
        fn name(&self) -> &str {
            self.0
        }
        async fn check(&self) -> HealthStatus {
            self.1
        }
    }

    fn check(name: &'static str, status: HealthStatus) -> Arc<dyn HealthCheck> {
        Arc::new(Static(name, status))
    }

    #[actix_web::test]
    async fn empty_registry_is_ready() {
        let report = HealthRegistry::new().report().await;
        assert!(report.is_ready());
        assert_eq!(report.http_status(), StatusCode::OK);
        assert!(report.checks.is_empty());
    }

    #[actix_web::test]
    async fn aggregates_to_worst_status() {
        let registry = HealthRegistry::new()
            .with_check(check("db", HealthStatus::Up))
            .with_check(check("cache", HealthStatus::Degraded));
        let report = registry.report().await;
        assert!(!report.is_ready());
        assert_eq!(report.status, HealthStatus::Degraded);
        assert_eq!(report.checks.len(), 2);
    }
}
