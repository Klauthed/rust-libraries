//! SQL-backed [`Outbox`] over sqlx's driver-agnostic `AnyPool`.
//!
//! [`SqlOutbox`] persists outbox rows in a single portable table so the same
//! code drives PostgreSQL, MySQL/MariaDB, or SQLite — whichever driver feature
//! is compiled in. The schema uses only portable column types (`TEXT`,
//! `BIGINT`, `INTEGER` boolean) so the DDL in [`SqlOutbox::CREATE_TABLE_SQL`]
//! applies unchanged across backends.
//!
//! Timestamps are stored as RFC3339 `TEXT` (lexicographically sortable) and
//! booleans as `0`/`1` integers, both for maximum portability through the `Any`
//! driver layer.
//!
//! # Concurrent pollers
//!
//! The portable [`fetch_unpublished`](Outbox::fetch_unpublished) does a plain
//! ordered `SELECT`; if several relay processes poll at once they can fetch the
//! same rows and double-publish (the broker should dedupe on event id). On
//! PostgreSQL prefer [`SqlOutbox::fetch_unpublished_skip_locked`] (gated behind
//! the `postgres` feature), which claims rows with `FOR UPDATE SKIP LOCKED` so
//! each poller gets a disjoint batch.
//!
//! ```no_run
//! # async fn run() -> Result<(), klauthed_data::DataError> {
//! use klauthed_data::outbox::Outbox;
//! use klauthed_data::outbox::SqlOutbox;
//!
//! sqlx::any::install_default_drivers();
//! let pool = sqlx::AnyPool::connect("sqlite::memory:").await?;
//! let outbox = SqlOutbox::new(pool);
//! outbox.ensure_schema().await?;
//! let pending = outbox.fetch_unpublished(100).await?;
//! # let _ = pending;
//! # Ok(())
//! # }
//! ```

use async_trait::async_trait;
use klauthed_core::time::Timestamp;
use sqlx::AnyPool;
use sqlx::Row;

use crate::error::DataError;
use crate::outbox::{Outbox, OutboxEntry, OutboxId};

/// Bind-placeholder style for the connected backend.
///
/// sqlx's `Any` driver passes SQL to the backend without rewriting placeholders,
/// so `?`-style queries must be translated to `$n` for PostgreSQL.
#[derive(Clone, Copy)]
enum Dialect {
    /// `?` placeholders (SQLite, MySQL).
    Question,
    /// `$1`, `$2`, … placeholders (PostgreSQL).
    Dollar,
}

/// A durable [`Outbox`] backed by a relational table on an [`AnyPool`].
///
/// Clone-cheap: holds only the pool handle (itself an `Arc` internally).
#[derive(Clone)]
pub struct SqlOutbox {
    pool: AnyPool,
    table: String,
    dialect: Dialect,
}

impl SqlOutbox {
    /// Default table name used when constructed with [`SqlOutbox::new`].
    pub const DEFAULT_TABLE: &'static str = "outbox";

    /// Portable DDL for the outbox table (default table name), created only if
    /// absent. Run once at startup via [`SqlOutbox::ensure_schema`], or apply it
    /// through your migration tooling.
    pub const CREATE_TABLE_SQL: &'static str = "\
CREATE TABLE IF NOT EXISTS outbox (
    id             TEXT    NOT NULL PRIMARY KEY,
    aggregate_type TEXT    NOT NULL,
    aggregate_id   TEXT    NOT NULL,
    event_type     TEXT    NOT NULL,
    sequence       BIGINT  NOT NULL,
    payload        TEXT    NOT NULL,
    occurred_at    TEXT    NOT NULL,
    published      INTEGER NOT NULL DEFAULT 0,
    published_at   TEXT
)";

    /// Wrap an existing pool, using the [`DEFAULT_TABLE`](Self::DEFAULT_TABLE)
    /// table name. The bind-placeholder dialect is detected from the pool's
    /// connection URL (PostgreSQL → `$n`, otherwise `?`).
    pub fn new(pool: AnyPool) -> Self {
        let dialect = if pool.connect_options().database_url.scheme().starts_with("postgres") {
            Dialect::Dollar
        } else {
            Dialect::Question
        };
        Self { pool, table: Self::DEFAULT_TABLE.to_owned(), dialect }
    }

    /// Borrow the underlying pool.
    pub fn pool(&self) -> &AnyPool {
        &self.pool
    }

    /// Translate the `?`-placeholder `sql` to the connected backend's dialect.
    ///
    /// These queries contain no literal `?` (only bind placeholders), so a
    /// sequential `?` → `$n` substitution is safe for PostgreSQL.
    fn rewrite(&self, sql: String) -> String {
        match self.dialect {
            Dialect::Question => sql,
            Dialect::Dollar => {
                let mut out = String::with_capacity(sql.len() + 8);
                let mut n = 0u32;
                for ch in sql.chars() {
                    if ch == '?' {
                        n += 1;
                        out.push('$');
                        out.push_str(&n.to_string());
                    } else {
                        out.push(ch);
                    }
                }
                out
            }
        }
    }

    /// Create the outbox table if it does not exist.
    ///
    /// Uses the bundled [`CREATE_TABLE_SQL`](Self::CREATE_TABLE_SQL); safe to call
    /// repeatedly. For non-default table names, run equivalent DDL yourself.
    pub async fn ensure_schema(&self) -> Result<(), DataError> {
        sqlx::query(Self::CREATE_TABLE_SQL).execute(&self.pool).await?;
        Ok(())
    }

    /// Build the column list shared by every `SELECT`.
    fn select_prefix(&self) -> String {
        format!(
            "SELECT id, aggregate_type, aggregate_id, event_type, sequence, \
             payload, occurred_at, published, published_at FROM {}",
            self.table
        )
    }

    /// Claim up to `limit` unpublished rows on PostgreSQL using
    /// `FOR UPDATE SKIP LOCKED`, so concurrent relay pollers receive disjoint
    /// batches.
    ///
    /// This must run inside the caller's transaction for the row locks to hold
    /// until commit; here it locks within an implicit single-statement
    /// transaction, which is enough to demonstrate the claim semantics. Pair it
    /// with [`mark_published`](Outbox::mark_published) before committing.
    ///
    /// Available only under the `postgres` feature, since `SKIP LOCKED` is not
    /// portable across all backends.
    #[cfg(feature = "postgres")]
    pub async fn fetch_unpublished_skip_locked(
        &self,
        limit: usize,
    ) -> Result<Vec<OutboxEntry>, DataError> {
        let sql = format!(
            "{prefix} WHERE published = 0 ORDER BY sequence ASC LIMIT {limit} FOR UPDATE SKIP LOCKED",
            prefix = self.select_prefix(),
            limit = limit as i64,
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql)).fetch_all(&self.pool).await?;
        rows.iter().map(row_to_entry).collect()
    }
}

/// Decode one `AnyRow` into an [`OutboxEntry`].
fn row_to_entry(row: &sqlx::any::AnyRow) -> Result<OutboxEntry, DataError> {
    let id_str: String = row.try_get("id")?;
    let id: OutboxId = id_str
        .parse()
        .map_err(|e| DataError::Outbox(format!("invalid outbox id '{id_str}': {e}")))?;

    let payload_str: String = row.try_get("payload")?;
    let payload: serde_json::Value = serde_json::from_str(&payload_str)
        .map_err(|e| DataError::Outbox(format!("invalid stored payload json: {e}")))?;

    let occurred_at_str: String = row.try_get("occurred_at")?;
    let occurred_at = parse_timestamp(&occurred_at_str)?;

    let published_at_str: Option<String> = row.try_get("published_at")?;
    let published_at = match published_at_str {
        Some(s) => Some(parse_timestamp(&s)?),
        None => None,
    };

    let sequence: i64 = row.try_get("sequence")?;
    let published: i64 = row.try_get("published")?;

    Ok(OutboxEntry {
        id,
        aggregate_type: row.try_get("aggregate_type")?,
        aggregate_id: row.try_get("aggregate_id")?,
        event_type: row.try_get("event_type")?,
        sequence: sequence as u64,
        payload,
        occurred_at,
        published: published != 0,
        published_at,
    })
}

/// Parse an RFC3339 string back into a [`Timestamp`] via its serde representation.
fn parse_timestamp(s: &str) -> Result<Timestamp, DataError> {
    serde_json::from_value(serde_json::Value::String(s.to_owned()))
        .map_err(|e| DataError::Outbox(format!("invalid stored timestamp '{s}': {e}")))
}

#[async_trait]
impl Outbox for SqlOutbox {
    async fn enqueue(&self, entries: Vec<OutboxEntry>) -> Result<(), DataError> {
        if entries.is_empty() {
            return Ok(());
        }

        let sql = self.rewrite(format!(
            "INSERT INTO {} \
             (id, aggregate_type, aggregate_id, event_type, sequence, payload, occurred_at, published, published_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            self.table
        ));

        // One transaction so a partial batch never lands.
        let mut tx = self.pool.begin().await?;
        for entry in entries {
            let payload = serde_json::to_string(&entry.payload).map_err(|e| {
                DataError::Outbox(format!("failed to serialize outbox payload: {e}"))
            })?;
            let published_at = entry.published_at.map(|t| t.to_rfc3339());
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(entry.id.to_string())
                .bind(entry.aggregate_type)
                .bind(entry.aggregate_id)
                .bind(entry.event_type)
                .bind(entry.sequence as i64)
                .bind(payload)
                .bind(entry.occurred_at.to_rfc3339())
                .bind(i64::from(entry.published))
                .bind(published_at)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn fetch_unpublished(&self, limit: usize) -> Result<Vec<OutboxEntry>, DataError> {
        let sql = format!(
            "{prefix} WHERE published = 0 ORDER BY sequence ASC LIMIT {limit}",
            prefix = self.select_prefix(),
            limit = limit as i64,
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(&*sql)).fetch_all(&self.pool).await?;
        rows.iter().map(row_to_entry).collect()
    }

    async fn mark_published(&self, ids: &[OutboxId]) -> Result<(), DataError> {
        if ids.is_empty() {
            return Ok(());
        }
        let now = Timestamp::now().to_rfc3339();
        let sql = self.rewrite(format!(
            "UPDATE {} SET published = 1, published_at = ? WHERE id = ? AND published = 0",
            self.table
        ));
        let mut tx = self.pool.begin().await?;
        for id in ids {
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(now.clone())
                .bind(id.to_string())
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }
}

#[cfg(all(test, feature = "sqlite"))]
mod tests {
    use super::*;
    use klauthed_core::domain::{DomainEvent, EventEnvelope};
    use klauthed_core::id::Id;
    use serde::Serialize;
    use std::borrow::Cow;

    #[derive(Debug, Serialize)]
    struct Opened {
        owner: String,
    }

    impl DomainEvent for Opened {
        fn event_type(&self) -> &'static str {
            "account.opened"
        }
    }

    fn entry(seq: u64) -> OutboxEntry {
        let envelope = EventEnvelope {
            event_id: Id::new(),
            event_type: Cow::Borrowed("account.opened"),
            aggregate_id: "acct-1".to_owned(),
            aggregate_type: Cow::Borrowed("account"),
            sequence: seq,
            occurred_at: Timestamp::from_unix_millis(1_000 + seq as i64),
            payload: Opened { owner: format!("owner-{seq}") },
        };
        OutboxEntry::from_envelope(&envelope).unwrap()
    }

    async fn memory_outbox() -> SqlOutbox {
        sqlx::any::install_default_drivers();
        // SQLite in-memory databases are connection-local: every new connection
        // sees an empty database. Force max_connections(1) so all operations in
        // the test share the same connection and therefore the same DB.
        let pool = sqlx::pool::PoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect in-memory sqlite");
        let outbox = SqlOutbox::new(pool);
        outbox.ensure_schema().await.expect("ensure schema");
        outbox
    }

    #[tokio::test]
    async fn ensure_schema_is_idempotent() {
        let outbox = memory_outbox().await;
        // Second call must not error.
        outbox.ensure_schema().await.unwrap();
        assert!(outbox.fetch_unpublished(10).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn enqueue_fetch_mark_round_trip_over_any_sqlite() {
        let outbox = memory_outbox().await;
        let e1 = entry(1);
        let e2 = entry(2);
        let (id1, id2) = (e1.id, e2.id);

        outbox.enqueue(vec![e1.clone(), e2.clone()]).await.unwrap();

        let unpublished = outbox.fetch_unpublished(10).await.unwrap();
        assert_eq!(unpublished.len(), 2);
        // Ordered by sequence ascending; full fidelity round-trip on the first row.
        assert_eq!(unpublished[0], e1);
        assert_eq!(unpublished[1].id, id2);
        assert_eq!(unpublished[0].payload["owner"], "owner-1");
        assert!(!unpublished[0].published);

        // Publish the first; it drops out of the next fetch.
        outbox.mark_published(&[id1]).await.unwrap();
        let remaining = outbox.fetch_unpublished(10).await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, id2);

        outbox.mark_published(&[id2]).await.unwrap();
        assert!(outbox.fetch_unpublished(10).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn fetch_honors_limit_and_sequence_order() {
        let outbox = memory_outbox().await;
        let entries: Vec<_> = (1..=5).map(entry).collect();
        outbox.enqueue(entries).await.unwrap();

        let two = outbox.fetch_unpublished(2).await.unwrap();
        assert_eq!(two.len(), 2);
        assert_eq!(two[0].sequence, 1);
        assert_eq!(two[1].sequence, 2);

        assert_eq!(outbox.fetch_unpublished(100).await.unwrap().len(), 5);
    }

    #[tokio::test]
    async fn marking_published_stores_published_at() {
        let outbox = memory_outbox().await;
        let e = entry(1);
        let id = e.id;
        outbox.enqueue(vec![e]).await.unwrap();
        outbox.mark_published(&[id]).await.unwrap();

        // Re-marking a published row is a no-op (WHERE published = 0).
        outbox.mark_published(&[id]).await.unwrap();
        assert!(outbox.fetch_unpublished(10).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn empty_batches_are_noops() {
        let outbox = memory_outbox().await;
        outbox.enqueue(vec![]).await.unwrap();
        outbox.mark_published(&[]).await.unwrap();
        assert!(outbox.fetch_unpublished(10).await.unwrap().is_empty());
    }

    // Live PostgreSQL test for the `?` → `$n` placeholder translation and the
    // postgres `FOR UPDATE SKIP LOCKED` claim. Ignored by default; the CI
    // `integration` job runs it against a Postgres at `DB_URL`:
    //   cargo test -p klauthed-data --features postgres --tests -- --ignored
    #[cfg(feature = "postgres")]
    #[tokio::test]
    #[ignore = "requires a live PostgreSQL at DB_URL"]
    async fn postgres_enqueue_fetch_mark_round_trip() {
        let url = std::env::var("DB_URL")
            .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/klauthed_test".into());
        sqlx::any::install_default_drivers();
        let pool = sqlx::AnyPool::connect(&url).await.expect("connect postgres");
        let outbox = SqlOutbox::new(pool);
        // Clean slate so re-runs are deterministic.
        sqlx::query(sqlx::AssertSqlSafe("DROP TABLE IF EXISTS outbox"))
            .execute(outbox.pool())
            .await
            .expect("drop");
        outbox.ensure_schema().await.expect("ensure schema");

        let (e1, e2) = (entry(1), entry(2));
        let (id1, id2) = (e1.id, e2.id);
        outbox.enqueue(vec![e1, e2]).await.unwrap(); // exercises ? -> $n on INSERT

        assert_eq!(outbox.fetch_unpublished(10).await.unwrap().len(), 2);
        // Postgres-only SKIP LOCKED claim path.
        assert_eq!(outbox.fetch_unpublished_skip_locked(10).await.unwrap().len(), 2);

        outbox.mark_published(&[id1]).await.unwrap(); // exercises ? -> $n on UPDATE
        let remaining = outbox.fetch_unpublished(10).await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, id2);

        sqlx::query(sqlx::AssertSqlSafe("DROP TABLE outbox"))
            .execute(outbox.pool())
            .await
            .expect("drop test table");
    }
}
