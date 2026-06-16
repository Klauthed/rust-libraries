//! Embedded, versioned schema migrations over a relational pool
//! (`feature = "sql"`).
//!
//! Define forward [`Migration`]s and run them with a [`Migrator`]; applied
//! versions are tracked in a `_klauthed_migrations` table so each runs exactly
//! once and re-running is a no-op. Works over the driver-agnostic
//! [`sqlx::AnyPool`], so the same runner serves Postgres / MySQL / SQLite — the
//! migration SQL is yours to keep portable to whichever you target.
//!
//! ```no_run
//! use klauthed_data::migrate::{Migration, Migrator};
//!
//! # async fn run(pool: &klauthed_data::AnyPool) -> Result<(), klauthed_data::DataError> {
//! let migrator = Migrator::new([
//!     Migration::new(1, "create_users", "CREATE TABLE users (id BIGINT PRIMARY KEY)"),
//!     Migration::new(2, "add_email", "ALTER TABLE users ADD COLUMN email TEXT"),
//! ])?;
//! let applied = migrator.run(pool).await?;
//! println!("applied {applied} migration(s)");
//! # Ok(())
//! # }
//! ```

use std::collections::BTreeSet;

use sqlx::{AnyPool, Row};

use crate::error::DataError;

/// A single forward ("up") migration.
#[derive(Debug, Clone)]
pub struct Migration {
    /// Monotonic version; migrations apply in ascending order and record once.
    pub version: i64,
    /// Human-readable name, stored alongside the version and logged.
    pub name: &'static str,
    /// The SQL to run. May contain multiple statements.
    pub sql: &'static str,
}

impl Migration {
    /// Construct a migration.
    #[must_use]
    pub const fn new(version: i64, name: &'static str, sql: &'static str) -> Self {
        Self { version, name, sql }
    }
}

/// Runs ordered [`Migration`]s against an [`AnyPool`], tracking applied versions
/// so each runs exactly once.
#[derive(Debug, Clone)]
pub struct Migrator {
    migrations: Vec<Migration>,
}

impl Migrator {
    /// Build a migrator from a set of migrations (sorted by version).
    ///
    /// # Errors
    /// Returns [`DataError::Migration`] if two migrations share a version.
    pub fn new(migrations: impl IntoIterator<Item = Migration>) -> Result<Self, DataError> {
        let mut migrations: Vec<Migration> = migrations.into_iter().collect();
        migrations.sort_by_key(|m| m.version);

        for pair in migrations.windows(2) {
            if let [a, b] = pair
                && a.version == b.version
            {
                return Err(DataError::Migration(format!(
                    "duplicate migration version {}",
                    a.version
                )));
            }
        }
        Ok(Self { migrations })
    }

    /// Apply every pending migration in version order; returns the number
    /// applied. Re-running after a successful run applies nothing.
    ///
    /// Each migration runs in its own transaction, so a failure leaves earlier
    /// migrations committed and the failing one rolled back.
    ///
    /// # Errors
    /// Returns [`DataError`] on any SQL failure.
    pub async fn run(&self, pool: &AnyPool) -> Result<u64, DataError> {
        ensure_table(pool).await?;
        let applied: BTreeSet<i64> = fetch_versions(pool).await?.into_iter().collect();

        let mut count = 0u64;
        for migration in &self.migrations {
            if applied.contains(&migration.version) {
                continue;
            }
            tracing::info!(
                version = migration.version,
                name = migration.name,
                "applying migration"
            );

            let mut tx = pool.begin().await?;
            sqlx::raw_sql(migration.sql).execute(&mut *tx).await?;
            let record = format!(
                "INSERT INTO _klauthed_migrations (version, name) VALUES ({}, '{}')",
                migration.version,
                migration.name.replace('\'', "''"),
            );
            // Audited: `version` is an integer and `name` is a `'static` literal
            // (single quotes escaped), so this inline write is injection-safe.
            sqlx::raw_sql(sqlx::AssertSqlSafe(record)).execute(&mut *tx).await?;
            tx.commit().await?;
            count += 1;
        }
        Ok(count)
    }

    /// The versions already recorded as applied, ascending.
    ///
    /// # Errors
    /// Returns [`DataError`] on a SQL failure.
    pub async fn applied(&self, pool: &AnyPool) -> Result<Vec<i64>, DataError> {
        ensure_table(pool).await?;
        fetch_versions(pool).await
    }
}

/// Create the migration-tracking table if it doesn't exist (portable DDL).
async fn ensure_table(pool: &AnyPool) -> Result<(), DataError> {
    sqlx::raw_sql(
        "CREATE TABLE IF NOT EXISTS _klauthed_migrations (\
         version BIGINT PRIMARY KEY, \
         name TEXT NOT NULL, \
         applied_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP)",
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Read the recorded versions in ascending order.
async fn fetch_versions(pool: &AnyPool) -> Result<Vec<i64>, DataError> {
    let rows = sqlx::query("SELECT version FROM _klauthed_migrations ORDER BY version")
        .fetch_all(pool)
        .await?;
    let mut versions = Vec::with_capacity(rows.len());
    for row in &rows {
        versions.push(row.try_get::<i64, _>("version")?);
    }
    Ok(versions)
}

#[cfg(all(test, feature = "sqlite"))]
mod tests {
    use super::*;

    async fn memory_pool() -> AnyPool {
        sqlx::any::install_default_drivers();
        // `sqlite::memory:` is private per connection, so pin the pool to one
        // connection — otherwise each query could hit a different empty database.
        sqlx::any::AnyPoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn applies_pending_then_is_idempotent() {
        let pool = memory_pool().await;
        let migrator = Migrator::new([
            Migration::new(1, "create_users", "CREATE TABLE users (id BIGINT PRIMARY KEY)"),
            Migration::new(2, "add_email", "ALTER TABLE users ADD COLUMN email TEXT"),
        ])
        .unwrap();

        assert_eq!(migrator.run(&pool).await.unwrap(), 2);
        assert_eq!(migrator.applied(&pool).await.unwrap(), vec![1, 2]);

        // A second run applies nothing.
        assert_eq!(migrator.run(&pool).await.unwrap(), 0);

        // The migrated schema is usable.
        sqlx::raw_sql("INSERT INTO users (id, email) VALUES (1, 'a@b.c')")
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn rejects_duplicate_versions() {
        let result =
            Migrator::new([Migration::new(1, "a", "SELECT 1"), Migration::new(1, "b", "SELECT 1")]);
        assert!(matches!(result, Err(DataError::Migration(_))));
    }
}
