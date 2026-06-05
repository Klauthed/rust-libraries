//! Relational database connections from a [`DatabaseConfig`].
//!
//! Uses sqlx's driver-agnostic [`AnyPool`], so one function connects PostgreSQL,
//! MySQL/MariaDB, or SQLite depending on which driver features are enabled and
//! what `DatabaseConfig::system` / the connection URL select at runtime.
//!
//! NoSQL systems (e.g. MongoDB) are not relational and are rejected here with
//! [`DataError::UnsupportedSystem`]; they get their own connector.

use std::time::Duration;

use klauthed_core::config::{DatabaseConfig, PoolConfig};
use sqlx::AnyPool;
use sqlx::any::AnyPoolOptions;

use crate::error::DataError;

/// Connect to a relational database described by `config`, returning a ready
/// connection pool.
///
/// The concrete driver is chosen from the connection URL scheme, so the matching
/// feature (`postgres` / `mysql` / `sqlite`) must be enabled or the connection
/// will fail at runtime with an "unsupported scheme" error from sqlx.
pub async fn connect(config: &DatabaseConfig) -> Result<AnyPool, DataError> {
    if !config.system.is_relational() {
        return Err(DataError::UnsupportedSystem(config.system));
    }

    // Registers whichever Any drivers were compiled in (idempotent).
    sqlx::any::install_default_drivers();

    let url = config.connection_url();
    tracing::debug!(system = ?config.system, "connecting to relational database");

    let pool = pool_options(&config.pool).connect(&url).await?;
    Ok(pool)
}

/// Connect and immediately verify the database answers, so misconfiguration
/// fails fast at startup rather than on the first query.
pub async fn connect_verified(config: &DatabaseConfig) -> Result<AnyPool, DataError> {
    let pool = connect(config).await?;
    ping(&pool).await?;
    Ok(pool)
}

/// Health-check an existing pool by issuing `SELECT 1`. Works across all
/// supported relational backends.
pub async fn ping(pool: &AnyPool) -> Result<(), DataError> {
    sqlx::query("SELECT 1").execute(pool).await?;
    Ok(())
}

/// Gracefully close a pool, waiting for in-flight connections to be released.
/// Call this during shutdown so the database sees a clean disconnect.
pub async fn close(pool: &AnyPool) {
    pool.close().await;
}

/// Translate our [`PoolConfig`] into sqlx [`AnyPoolOptions`].
fn pool_options(pool: &PoolConfig) -> AnyPoolOptions {
    let mut options = AnyPoolOptions::new()
        .max_connections(pool.max_connections)
        .min_connections(pool.min_connections)
        .acquire_timeout(Duration::from_secs(pool.acquire_timeout_secs));

    if let Some(idle) = pool.idle_timeout_secs {
        options = options.idle_timeout(Duration::from_secs(idle));
    }
    if let Some(lifetime) = pool.max_lifetime_secs {
        options = options.max_lifetime(Duration::from_secs(lifetime));
    }
    options
}

#[cfg(test)]
mod tests {
    use super::*;
    use klauthed_core::config::DbSystem;

    #[tokio::test]
    async fn rejects_non_relational_system() {
        let config = DatabaseConfig {
            system: DbSystem::MongoDb,
            ..Default::default()
        };
        let err = connect(&config).await.unwrap_err();
        assert!(matches!(err, DataError::UnsupportedSystem(DbSystem::MongoDb)));
    }

    #[cfg(feature = "sqlite")]
    #[tokio::test]
    async fn connect_verify_ping_close_against_sqlite_memory() {
        // A real end-to-end exercise of the round-out: connect, SELECT 1, close.
        let config = DatabaseConfig {
            system: DbSystem::Sqlite,
            url: Some("sqlite::memory:".into()),
            ..Default::default()
        };

        let pool = connect_verified(&config).await.expect("connect + ping");
        ping(&pool).await.expect("ping again");
        close(&pool).await;
    }
}
