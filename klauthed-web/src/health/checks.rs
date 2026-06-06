//! Concrete `HealthCheck` implementations for common infrastructure.
//!
//! Each check is feature-gated so only the ones you actually use pull in the
//! corresponding driver crate.

#[cfg(any(feature = "data-sql", feature = "data-redis"))]
use super::{HealthCheck, HealthStatus};

// ── SqlHealthCheck ────────────────────────────────────────────────────────────

/// A [`HealthCheck`] that probes a relational database by running `SELECT 1`.
///
/// Requires feature `data-sql`.
///
/// ```no_run
/// use std::sync::Arc;
/// use klauthed_web::health::{HealthRegistry, SqlHealthCheck};
///
/// # let pool: sqlx::AnyPool = todo!();
/// let registry = HealthRegistry::new()
///     .with_check(Arc::new(SqlHealthCheck::new("primary", pool)));
/// ```
#[cfg(feature = "data-sql")]
pub struct SqlHealthCheck {
    name: String,
    pool: sqlx::AnyPool,
}

#[cfg(feature = "data-sql")]
impl SqlHealthCheck {
    /// Create a new check with the given `name` and `pool`.
    pub fn new(name: impl Into<String>, pool: sqlx::AnyPool) -> Self {
        Self { name: name.into(), pool }
    }
}

#[cfg(feature = "data-sql")]
#[async_trait::async_trait]
impl HealthCheck for SqlHealthCheck {
    fn name(&self) -> &str {
        &self.name
    }

    async fn check(&self) -> HealthStatus {
        match sqlx::query("SELECT 1").execute(&self.pool).await {
            Ok(_) => HealthStatus::Up,
            Err(e) => {
                tracing::warn!(name = %self.name, error = %e, "database health check failed");
                HealthStatus::Down
            }
        }
    }
}

// ── RedisHealthCheck ──────────────────────────────────────────────────────────

/// A [`HealthCheck`] that pings a Redis server.
///
/// Requires feature `data-redis`.
///
/// ```no_run
/// use std::sync::Arc;
/// use klauthed_web::health::{HealthRegistry, RedisHealthCheck};
///
/// # let conn: redis::aio::ConnectionManager = todo!();
/// let registry = HealthRegistry::new()
///     .with_check(Arc::new(RedisHealthCheck::new("cache", conn)));
/// ```
#[cfg(feature = "data-redis")]
pub struct RedisHealthCheck {
    name: String,
    conn: redis::aio::ConnectionManager,
}

#[cfg(feature = "data-redis")]
impl RedisHealthCheck {
    /// Create a new check with the given `name` and connection manager.
    pub fn new(name: impl Into<String>, conn: redis::aio::ConnectionManager) -> Self {
        Self { name: name.into(), conn }
    }
}

#[cfg(feature = "data-redis")]
#[async_trait::async_trait]
impl HealthCheck for RedisHealthCheck {
    fn name(&self) -> &str {
        &self.name
    }

    async fn check(&self) -> HealthStatus {
        let mut conn = self.conn.clone();
        match redis::cmd("PING").query_async::<String>(&mut conn).await {
            Ok(_) => HealthStatus::Up,
            Err(e) => {
                tracing::warn!(name = %self.name, error = %e, "redis health check failed");
                HealthStatus::Down
            }
        }
    }
}

// ── Static Send+Sync assertions ───────────────────────────────────────────────

#[cfg(all(test, feature = "data-sql"))]
const _: fn() = || {
    fn assert_send_sync<T: Send + Sync + 'static>() {}
    assert_send_sync::<SqlHealthCheck>();
};

#[cfg(all(test, feature = "data-redis"))]
const _: fn() = || {
    fn assert_send_sync<T: Send + Sync + 'static>() {}
    assert_send_sync::<RedisHealthCheck>();
};

// ── Live integration tests (need real infra — run with --ignored) ─────────────

#[cfg(all(test, feature = "data-sql"))]
#[ignore]
#[actix_web::test]
async fn sql_health_check_live_up() {
    let url = std::env::var("DB_URL").expect("DB_URL must be set");
    sqlx::any::install_default_drivers();
    let pool = sqlx::AnyPool::connect(&url).await.expect("pool");
    let check = SqlHealthCheck::new("db", pool);
    assert_eq!(check.check().await, HealthStatus::Up);
}

#[cfg(all(test, feature = "data-redis"))]
#[ignore]
#[actix_web::test]
async fn redis_health_check_live_up() {
    let url = std::env::var("REDIS_URL").expect("REDIS_URL must be set");
    let client = redis::Client::open(url).expect("client");
    let conn = redis::aio::ConnectionManager::new(client).await.expect("conn");
    let check = RedisHealthCheck::new("redis", conn);
    assert_eq!(check.check().await, HealthStatus::Up);
}
