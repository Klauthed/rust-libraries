//! [`SqlJobQueue`]: a durable [`JobQueue`] backed by a relational table.
//!
//! Persists jobs in a single portable `jobs` table over sqlx's driver-agnostic
//! [`AnyPool`], so the same code drives PostgreSQL, MySQL/MariaDB, or SQLite. The
//! schema uses only portable column types (`TEXT`, `BIGINT`); timestamps are
//! stored as Unix-epoch milliseconds and `status` / `payload` as text. All timing
//! decisions read an injected [`Clock`], matching
//! [`InMemoryJobQueue`](super::InMemoryJobQueue).
//!
//! sqlx's `Any` driver does not rewrite bind placeholders, so queries are written
//! with `?` and translated to `$n` when the pool is PostgreSQL (detected from the
//! connection URL at construction).
//!
//! # Concurrent workers
//!
//! The portable [`dequeue_due`](JobQueue::dequeue_due) claims jobs with a
//! `SELECT` then `UPDATE` inside one transaction; if several workers poll at once
//! they can claim the same row. On PostgreSQL prefer
//! [`dequeue_due_skip_locked`](SqlJobQueue::dequeue_due_skip_locked), which uses
//! `FOR UPDATE SKIP LOCKED` so each worker gets a disjoint batch.
//!
//! ```no_run
//! # async fn run() -> Result<(), klauthed_platform::PlatformError> {
//! use std::sync::Arc;
//! use klauthed_core::time::SystemClock;
//! use klauthed_platform::{JobQueue, SqlJobQueue};
//!
//! sqlx::any::install_default_drivers();
//! let pool = sqlx::AnyPool::connect("sqlite::memory:").await.unwrap();
//! let queue = SqlJobQueue::new(pool, Arc::new(SystemClock));
//! queue.ensure_schema().await?;
//! let job = queue.enqueue("send_email".into(), serde_json::json!({ "to": "a@b.com" })).await?;
//! # let _ = job;
//! # Ok(())
//! # }
//! ```

use std::sync::Arc;

use async_trait::async_trait;
use klauthed_core::time::{Clock, Duration, Timestamp};
use sqlx::{AnyPool, Row};

use super::queue::backoff_for_attempt;
use super::{DEFAULT_MAX_ATTEMPTS, EnqueuedJob, JobId, JobQueue, JobStatus};
use crate::error::PlatformError;

/// Bind-placeholder style for the connected backend.
#[derive(Clone, Copy)]
enum Dialect {
    /// `?` placeholders (SQLite, MySQL).
    Question,
    /// `$1`, `$2`, … placeholders (PostgreSQL).
    Dollar,
}

/// Map a sqlx error to a [`PlatformError::Backend`].
fn db_err(error: sqlx::Error) -> PlatformError {
    PlatformError::Backend { message: format!("job queue: {error}") }
}

/// The stored text form of a [`JobStatus`].
fn status_str(status: JobStatus) -> &'static str {
    match status {
        JobStatus::Queued => "queued",
        JobStatus::Running => "running",
        JobStatus::Succeeded => "succeeded",
        JobStatus::Failed => "failed",
    }
}

/// Parse a stored status string back into a [`JobStatus`].
fn parse_status(s: &str) -> Result<JobStatus, PlatformError> {
    match s {
        "queued" => Ok(JobStatus::Queued),
        "running" => Ok(JobStatus::Running),
        "succeeded" => Ok(JobStatus::Succeeded),
        "failed" => Ok(JobStatus::Failed),
        other => Err(PlatformError::Backend { message: format!("unknown job status '{other}'") }),
    }
}

/// Decode one row into an [`EnqueuedJob`].
fn row_to_job(row: &sqlx::any::AnyRow) -> Result<EnqueuedJob, PlatformError> {
    let id_str: String = row.try_get("id").map_err(db_err)?;
    let id: JobId = id_str.parse().map_err(|e| PlatformError::Backend {
        message: format!("invalid job id '{id_str}': {e}"),
    })?;

    let payload_str: String = row.try_get("payload").map_err(db_err)?;
    let payload: serde_json::Value = serde_json::from_str(&payload_str).map_err(|e| {
        PlatformError::Backend { message: format!("invalid job payload json: {e}") }
    })?;

    let status_str: String = row.try_get("status").map_err(db_err)?;
    let run_at: i64 = row.try_get("run_at").map_err(db_err)?;
    let created_at: i64 = row.try_get("created_at").map_err(db_err)?;
    let attempts: i64 = row.try_get("attempts").map_err(db_err)?;
    let max_attempts: i64 = row.try_get("max_attempts").map_err(db_err)?;

    Ok(EnqueuedJob {
        id,
        kind: row.try_get("kind").map_err(db_err)?,
        payload,
        run_at: Timestamp::from_unix_millis(run_at),
        attempts: u32::try_from(attempts).unwrap_or(u32::MAX),
        max_attempts: u32::try_from(max_attempts).unwrap_or(u32::MAX),
        status: parse_status(&status_str)?,
        created_at: Timestamp::from_unix_millis(created_at),
        last_error: row.try_get("last_error").map_err(db_err)?,
    })
}

/// A durable [`JobQueue`] backed by a relational `jobs` table on an [`AnyPool`].
///
/// Clone-cheap: holds only the pool handle (an `Arc` internally) and the clock.
#[derive(Clone)]
pub struct SqlJobQueue {
    pool: AnyPool,
    clock: Arc<dyn Clock>,
    default_max_attempts: u32,
    table: String,
    dialect: Dialect,
}

impl SqlJobQueue {
    /// Default table name used by [`SqlJobQueue::new`].
    pub const DEFAULT_TABLE: &'static str = "jobs";

    /// Wrap a pool, using the [`DEFAULT_TABLE`](Self::DEFAULT_TABLE) and
    /// [`DEFAULT_MAX_ATTEMPTS`] for newly enqueued jobs. `clock` drives all timing.
    ///
    /// The bind-placeholder dialect is detected from the pool's connection URL
    /// (PostgreSQL → `$n`, otherwise `?`).
    #[must_use]
    pub fn new(pool: AnyPool, clock: Arc<dyn Clock>) -> Self {
        let dialect = if pool.connect_options().database_url.scheme().starts_with("postgres") {
            Dialect::Dollar
        } else {
            Dialect::Question
        };
        Self {
            pool,
            clock,
            default_max_attempts: DEFAULT_MAX_ATTEMPTS,
            table: Self::DEFAULT_TABLE.to_owned(),
            dialect,
        }
    }

    /// Override the default attempt cap (clamped to at least 1).
    #[must_use]
    pub fn with_max_attempts(mut self, max_attempts: u32) -> Self {
        self.default_max_attempts = max_attempts.max(1);
        self
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

    /// Create the jobs table and its index if absent. Safe to call repeatedly.
    ///
    /// # Errors
    /// Returns [`PlatformError::Backend`] if the DDL fails.
    pub async fn ensure_schema(&self) -> Result<(), PlatformError> {
        let create_table = format!(
            "CREATE TABLE IF NOT EXISTS {} (\
                id TEXT NOT NULL PRIMARY KEY, \
                kind TEXT NOT NULL, \
                payload TEXT NOT NULL, \
                run_at BIGINT NOT NULL, \
                attempts BIGINT NOT NULL, \
                max_attempts BIGINT NOT NULL, \
                status TEXT NOT NULL, \
                created_at BIGINT NOT NULL, \
                last_error TEXT)",
            self.table
        );
        let create_index = format!(
            "CREATE INDEX IF NOT EXISTS {table}_status_run_at ON {table} (status, run_at)",
            table = self.table
        );
        sqlx::query(sqlx::AssertSqlSafe(create_table)).execute(&self.pool).await.map_err(db_err)?;
        sqlx::query(sqlx::AssertSqlSafe(create_index)).execute(&self.pool).await.map_err(db_err)?;
        Ok(())
    }

    async fn insert(
        &self,
        kind: String,
        payload: serde_json::Value,
        run_at: Timestamp,
    ) -> Result<EnqueuedJob, PlatformError> {
        let job = EnqueuedJob {
            id: JobId::new(),
            kind,
            payload,
            run_at,
            attempts: 0,
            max_attempts: self.default_max_attempts,
            status: JobStatus::Queued,
            created_at: self.clock.now(),
            last_error: None,
        };
        let payload_str = serde_json::to_string(&job.payload).map_err(|e| {
            PlatformError::Backend { message: format!("serialize job payload: {e}") }
        })?;
        let sql = self.rewrite(format!(
            "INSERT INTO {} (id, kind, payload, run_at, attempts, max_attempts, status, created_at, last_error) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            self.table
        ));
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(job.id.to_string())
            .bind(job.kind.clone())
            .bind(payload_str)
            .bind(job.run_at.unix_millis())
            .bind(i64::from(job.attempts))
            .bind(i64::from(job.max_attempts))
            .bind(status_str(job.status).to_owned())
            .bind(job.created_at.unix_millis())
            .bind(Option::<String>::None)
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(job)
    }

    /// The shared `SELECT` column list.
    fn select_columns(&self) -> String {
        format!(
            "SELECT id, kind, payload, run_at, attempts, max_attempts, status, created_at, last_error FROM {}",
            self.table
        )
    }

    /// Claim due jobs, marking each `Running` and bumping its attempt count, with
    /// PostgreSQL `FOR UPDATE SKIP LOCKED` so concurrent workers get disjoint
    /// batches. Postgres-only; on other backends use
    /// [`dequeue_due`](JobQueue::dequeue_due).
    ///
    /// # Errors
    /// Returns [`PlatformError::Backend`] if the query fails.
    pub async fn dequeue_due_skip_locked(
        &self,
        limit: Option<usize>,
    ) -> Result<Vec<EnqueuedJob>, PlatformError> {
        self.claim(limit, true).await
    }

    /// Shared claim implementation. `skip_locked` adds `FOR UPDATE SKIP LOCKED`
    /// (PostgreSQL only).
    async fn claim(
        &self,
        limit: Option<usize>,
        skip_locked: bool,
    ) -> Result<Vec<EnqueuedJob>, PlatformError> {
        let now = self.clock.now();
        let limit_clause = match limit {
            Some(n) => format!(" LIMIT {}", i64::try_from(n).unwrap_or(i64::MAX)),
            None => String::new(),
        };
        let lock_clause = if skip_locked { " FOR UPDATE SKIP LOCKED" } else { "" };
        let select_sql = self.rewrite(format!(
            "{cols} WHERE status = 'queued' AND run_at <= ? ORDER BY run_at ASC, id ASC{limit}{lock}",
            cols = self.select_columns(),
            limit = limit_clause,
            lock = lock_clause,
        ));
        let update_sql = self.rewrite(format!(
            "UPDATE {} SET status = 'running', attempts = attempts + 1 WHERE id = ?",
            self.table
        ));

        let mut tx = self.pool.begin().await.map_err(db_err)?;
        let rows = sqlx::query(sqlx::AssertSqlSafe(select_sql))
            .bind(now.unix_millis())
            .fetch_all(&mut *tx)
            .await
            .map_err(db_err)?;

        let mut claimed = Vec::with_capacity(rows.len());
        for row in &rows {
            let mut job = row_to_job(row)?;
            sqlx::query(sqlx::AssertSqlSafe(update_sql.clone()))
                .bind(job.id.to_string())
                .execute(&mut *tx)
                .await
                .map_err(db_err)?;
            job.status = JobStatus::Running;
            job.attempts += 1;
            claimed.push(job);
        }
        tx.commit().await.map_err(db_err)?;
        Ok(claimed)
    }
}

#[async_trait]
impl JobQueue for SqlJobQueue {
    async fn enqueue(
        &self,
        kind: String,
        payload: serde_json::Value,
    ) -> Result<EnqueuedJob, PlatformError> {
        let now = self.clock.now();
        self.insert(kind, payload, now).await
    }

    async fn schedule(
        &self,
        kind: String,
        payload: serde_json::Value,
        run_at: Timestamp,
    ) -> Result<EnqueuedJob, PlatformError> {
        self.insert(kind, payload, run_at).await
    }

    async fn dequeue_due(&self, limit: Option<usize>) -> Result<Vec<EnqueuedJob>, PlatformError> {
        self.claim(limit, false).await
    }

    async fn mark_succeeded(&self, id: JobId) -> Result<(), PlatformError> {
        let sql = self.rewrite(format!(
            "UPDATE {} SET status = 'succeeded', last_error = NULL WHERE id = ?",
            self.table
        ));
        let result = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        if result.rows_affected() == 0 {
            return Err(PlatformError::JobNotFound { id: id.to_string() });
        }
        Ok(())
    }

    async fn mark_failed(&self, id: JobId, reason: String) -> Result<(), PlatformError> {
        let now = self.clock.now();
        let mut tx = self.pool.begin().await.map_err(db_err)?;

        let select_sql =
            self.rewrite(format!("SELECT attempts, max_attempts FROM {} WHERE id = ?", self.table));
        let row = sqlx::query(sqlx::AssertSqlSafe(select_sql))
            .bind(id.to_string())
            .fetch_optional(&mut *tx)
            .await
            .map_err(db_err)?;
        let Some(row) = row else {
            return Err(PlatformError::JobNotFound { id: id.to_string() });
        };
        let attempts: i64 = row.try_get("attempts").map_err(db_err)?;
        let max_attempts: i64 = row.try_get("max_attempts").map_err(db_err)?;

        if attempts >= max_attempts {
            let sql = self.rewrite(format!(
                "UPDATE {} SET status = 'failed', last_error = ? WHERE id = ?",
                self.table
            ));
            sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(reason)
                .bind(id.to_string())
                .execute(&mut *tx)
                .await
                .map_err(db_err)?;
        } else {
            let delay = backoff_for_attempt(u32::try_from(attempts).unwrap_or(u32::MAX));
            let new_run_at = now.checked_add(delay).unwrap_or(now).unix_millis();
            let sql = self.rewrite(format!(
                "UPDATE {} SET status = 'queued', run_at = ?, last_error = ? WHERE id = ?",
                self.table
            ));
            sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(new_run_at)
                .bind(reason)
                .bind(id.to_string())
                .execute(&mut *tx)
                .await
                .map_err(db_err)?;
        }
        tx.commit().await.map_err(db_err)?;
        Ok(())
    }

    async fn dequeue_stalled(
        &self,
        stall_after: Duration,
    ) -> Result<Vec<EnqueuedJob>, PlatformError> {
        let now = self.clock.now();
        let stall_ms = i64::try_from(stall_after.whole_milliseconds()).unwrap_or(i64::MAX);
        // Stalled when now - run_at > stall_after, i.e. run_at < now - stall_after.
        let cutoff = now.unix_millis().saturating_sub(stall_ms);

        let mut tx = self.pool.begin().await.map_err(db_err)?;
        let select_sql = self.rewrite(format!(
            "{cols} WHERE status = 'running' AND run_at < ?",
            cols = self.select_columns(),
        ));
        let rows = sqlx::query(sqlx::AssertSqlSafe(select_sql))
            .bind(cutoff)
            .fetch_all(&mut *tx)
            .await
            .map_err(db_err)?;

        let update_sql = self.rewrite(format!(
            "UPDATE {} SET status = 'queued', run_at = ? WHERE id = ?",
            self.table
        ));
        let mut recovered = Vec::with_capacity(rows.len());
        for row in &rows {
            let mut job = row_to_job(row)?;
            sqlx::query(sqlx::AssertSqlSafe(update_sql.clone()))
                .bind(now.unix_millis())
                .bind(job.id.to_string())
                .execute(&mut *tx)
                .await
                .map_err(db_err)?;
            job.status = JobStatus::Queued;
            job.run_at = now;
            recovered.push(job);
        }
        tx.commit().await.map_err(db_err)?;
        Ok(recovered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use klauthed_core::time::FixedClock;

    async fn memory_queue() -> SqlJobQueue {
        sqlx::any::install_default_drivers();
        // A single shared connection so the in-memory SQLite db persists across ops.
        let pool = sqlx::any::AnyPoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect in-memory sqlite");
        let clock = Arc::new(FixedClock::at_unix_millis(0));
        let queue = SqlJobQueue::new(pool, clock).with_max_attempts(3);
        queue.ensure_schema().await.expect("ensure schema");
        queue
    }

    /// A single-connection in-memory queue sharing `clock` for time control.
    async fn queue_with_clock(clock: Arc<FixedClock>) -> SqlJobQueue {
        sqlx::any::install_default_drivers();
        let pool = sqlx::any::AnyPoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let queue = SqlJobQueue::new(pool, clock).with_max_attempts(2);
        queue.ensure_schema().await.unwrap();
        queue
    }

    #[tokio::test]
    async fn ensure_schema_is_idempotent() {
        let queue = memory_queue().await;
        queue.ensure_schema().await.unwrap();
        assert!(queue.dequeue_due(None).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn enqueue_then_dequeue_due_marks_running() {
        let queue = memory_queue().await;
        let job = queue.enqueue("k".into(), serde_json::json!({ "a": 1 })).await.unwrap();
        assert_eq!(job.status(), JobStatus::Queued);

        let due = queue.dequeue_due(None).await.unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id(), job.id());
        assert_eq!(due[0].status(), JobStatus::Running);
        assert_eq!(due[0].attempts(), 1);
        assert_eq!(due[0].payload()["a"], 1);

        // Claimed jobs are no longer due.
        assert!(queue.dequeue_due(None).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn scheduled_job_is_not_due_until_clock_advances() {
        let clock = Arc::new(FixedClock::at_unix_millis(0));
        let queue = queue_with_clock(clock.clone()).await;

        let run_at = clock.now().checked_add(Duration::seconds(60)).unwrap();
        let job = queue.schedule("k".into(), serde_json::json!(null), run_at).await.unwrap();
        assert!(queue.dequeue_due(None).await.unwrap().is_empty());

        clock.advance(Duration::seconds(61));
        let due = queue.dequeue_due(None).await.unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id(), job.id());
    }

    #[tokio::test]
    async fn mark_succeeded_is_terminal() {
        let queue = memory_queue().await;
        let job = queue.enqueue("k".into(), serde_json::json!(null)).await.unwrap();
        queue.dequeue_due(None).await.unwrap();
        queue.mark_succeeded(job.id()).await.unwrap();
        assert!(queue.dequeue_due(None).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn mark_unknown_job_is_not_found() {
        let queue = memory_queue().await;
        let err = queue.mark_succeeded(JobId::new()).await.unwrap_err();
        assert!(matches!(err, PlatformError::JobNotFound { .. }));
    }

    #[tokio::test]
    async fn mark_failed_requeues_with_backoff_then_fails_at_max() {
        let clock = Arc::new(FixedClock::at_unix_millis(0));
        let queue = queue_with_clock(clock.clone()).await; // max_attempts = 2

        let job = queue.enqueue("k".into(), serde_json::json!(null)).await.unwrap();

        // Attempt 1 → failure re-queues with a 1s backoff.
        queue.dequeue_due(None).await.unwrap();
        queue.mark_failed(job.id(), "boom-1".into()).await.unwrap();
        assert!(queue.dequeue_due(None).await.unwrap().is_empty(), "backoff not yet elapsed");

        // Attempt 2 (== max) → terminal Failed.
        clock.advance(Duration::seconds(2));
        let due = queue.dequeue_due(None).await.unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].attempts(), 2);
        queue.mark_failed(job.id(), "boom-2".into()).await.unwrap();

        // Failed and never due again.
        clock.advance(Duration::seconds(3600));
        assert!(queue.dequeue_due(None).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn dequeue_stalled_recovers_running_jobs_past_the_window() {
        let clock = Arc::new(FixedClock::at_unix_millis(0));
        let queue = queue_with_clock(clock.clone()).await;

        let job = queue.enqueue("k".into(), serde_json::json!(null)).await.unwrap();
        queue.dequeue_due(None).await.unwrap(); // -> Running at run_at = 0

        // Within the window: not recovered.
        clock.advance(Duration::seconds(30));
        assert!(queue.dequeue_stalled(Duration::seconds(30)).await.unwrap().is_empty());

        // Past the window: recovered to Queued, immediately due again.
        clock.advance(Duration::seconds(1));
        let recovered = queue.dequeue_stalled(Duration::seconds(30)).await.unwrap();
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].id(), job.id());
        assert_eq!(recovered[0].status(), JobStatus::Queued);

        let due = queue.dequeue_due(None).await.unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id(), job.id());
    }

    // Live PostgreSQL test for the `FOR UPDATE SKIP LOCKED` claim path (and the
    // `?` → `$n` placeholder translation). Ignored by default; the CI
    // `integration` job runs it against a Postgres at `DB_URL`:
    //   cargo test -p klauthed-platform --features jobs-sql --tests -- --ignored
    #[tokio::test]
    #[ignore = "requires a live PostgreSQL at DB_URL"]
    async fn postgres_skip_locked_claim_round_trip() {
        let url = std::env::var("DB_URL")
            .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/klauthed_test".into());
        sqlx::any::install_default_drivers();
        let pool = sqlx::AnyPool::connect(&url).await.expect("connect postgres");

        let clock = Arc::new(FixedClock::at_unix_millis(0));
        let queue = SqlJobQueue::new(pool, clock).with_max_attempts(3);
        // Clean slate so re-runs are deterministic.
        sqlx::query(sqlx::AssertSqlSafe("DROP TABLE IF EXISTS jobs"))
            .execute(queue.pool())
            .await
            .expect("drop");
        queue.ensure_schema().await.expect("ensure schema");

        let job = queue.enqueue("k".into(), serde_json::json!({ "n": 1 })).await.unwrap();
        let due = queue.dequeue_due_skip_locked(Some(10)).await.unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id(), job.id());
        assert_eq!(due[0].status(), JobStatus::Running);
        // A second concurrent-style claim sees nothing (the row is now Running).
        assert!(queue.dequeue_due_skip_locked(Some(10)).await.unwrap().is_empty());

        // mark_failed re-queues (attempt 1 < max 3) with a 1s backoff: not yet due.
        queue.mark_failed(job.id(), "boom".into()).await.unwrap();
        assert!(queue.dequeue_due(None).await.unwrap().is_empty(), "re-queued with backoff");

        queue.mark_succeeded(job.id()).await.unwrap();
        sqlx::query(sqlx::AssertSqlSafe("DROP TABLE jobs"))
            .execute(queue.pool())
            .await
            .expect("drop test table");
    }
}
