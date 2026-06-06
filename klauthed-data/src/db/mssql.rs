//! SQL Server (MSSQL) connection pool via tiberius + bb8.
//!
//! # Connection string format
//!
//! [`DatabaseConfig::url`] must be an ADO.NET connection string understood by
//! [`tiberius::Config::from_ado_string`].  A minimal example:
//!
//! ```text
//! Server=tcp:localhost,1433;Database=mydb;User Id=sa;Password=secret;TrustServerCertificate=true
//! ```
//!
//! See the [tiberius documentation](https://docs.rs/tiberius) for the full
//! list of supported keys.
//!
//! Live tests are marked `#[ignore]`; run them with a SQL Server instance
//! available at `MSSQL_URL` via:
//! ```text
//! cargo test -p klauthed-data --features mssql -- --ignored
//! ```

use klauthed_core::config::DatabaseConfig;

use crate::error::DataError;

/// A bb8 pool backed by a tiberius MSSQL connection manager.
pub type MssqlPool = bb8::Pool<bb8_tiberius::ConnectionManager>;

/// Build an MSSQL connection pool from `config`.
///
/// `config.url` must be an ADO.NET connection string; the pool is sized to
/// `config.pool.max_connections`.
pub async fn connect(config: &DatabaseConfig) -> Result<MssqlPool, DataError> {
    let ado = config.url.as_deref().ok_or(DataError::MissingUrl("mssql"))?;

    let tiberius_cfg = tiberius::Config::from_ado_string(ado)
        .map_err(|e| DataError::Outbox(format!("mssql config parse error: {e}")))?;

    let manager = bb8_tiberius::ConnectionManager::new(tiberius_cfg);

    let pool = bb8::Pool::builder()
        .max_size(config.pool.max_connections)
        .build(manager)
        .await
        .map_err(|e| DataError::Outbox(format!("mssql pool error: {e}")))?;

    Ok(pool)
}

/// Build a pool and immediately verify connectivity with a `SELECT 1`.
pub async fn connect_verified(config: &DatabaseConfig) -> Result<MssqlPool, DataError> {
    let pool = connect(config).await?;
    ping(&pool).await?;
    Ok(pool)
}

/// Health-check an existing pool by running `SELECT 1`.
pub async fn ping(pool: &MssqlPool) -> Result<(), DataError> {
    let mut conn = pool
        .get()
        .await
        .map_err(|e| DataError::Outbox(format!("mssql acquire connection: {e}")))?;

    conn.simple_query("SELECT 1")
        .await
        .map_err(|e| DataError::Outbox(format!("mssql ping failed: {e}")))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use klauthed_core::config::DatabaseConfig;

    #[tokio::test]
    #[ignore = "requires a live SQL Server at MSSQL_URL"]
    async fn connect_verified_and_ping() {
        let url = std::env::var("MSSQL_URL").expect("MSSQL_URL must be set");
        let config = DatabaseConfig { url: Some(url), ..Default::default() };
        let pool = connect_verified(&config).await.expect("connect+ping");
        ping(&pool).await.expect("second ping");
    }
}
