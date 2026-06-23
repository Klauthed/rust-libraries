//! [`RedisJobQueue`]: a durable [`JobQueue`] backed by Redis.
//!
//! Each job is a Redis **hash** at `{prefix}:j:{id}` (fields `kind`, `payload`,
//! `run_at`, `attempts`, `max_attempts`, `status`, `created_at`, `last_error`).
//! Two sorted sets index lifecycle by time-in-milliseconds:
//!
//! * `{prefix}:due` — queued jobs scored by `run_at` (claimable once `score <= now`).
//! * `{prefix}:run` — running jobs scored by their `run_at`, so stalled jobs are
//!   found by score, matching [`InMemoryJobQueue`](super::InMemoryJobQueue).
//!
//! Claiming, failing, and stall-recovery run as atomic Lua scripts so concurrent
//! workers never double-claim. Timing reads an injected [`Clock`]; the retry
//! backoff mirrors the in-memory queue (`1s · 2^(attempts-1)`, capped at 1h).
//!
//! Not suitable for Redis Cluster as written (a script touches per-job keys
//! derived inside Lua, which must hash-slot together).

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use klauthed_core::time::{Clock, Duration, Timestamp};
use redis::AsyncCommands;
use redis::aio::ConnectionManager;

use super::{DEFAULT_MAX_ATTEMPTS, EnqueuedJob, JobId, JobQueue, JobStatus};
use crate::error::PlatformError;

/// Atomically claim due jobs: move each from `due` to `run` (preserving its
/// `run_at` score), mark it `running`, and bump `attempts`. Returns claimed ids.
/// `KEYS[1]`=due, `KEYS[2]`=run; `ARGV[1]`=now, `ARGV[2]`=limit (`-1` = no limit),
/// `ARGV[3]`=job-key prefix.
const CLAIM_SCRIPT: &str = r"
local limit = tonumber(ARGV[2])
local items
if limit < 0 then
  items = redis.call('ZRANGEBYSCORE', KEYS[1], '-inf', ARGV[1], 'WITHSCORES')
else
  items = redis.call('ZRANGEBYSCORE', KEYS[1], '-inf', ARGV[1], 'WITHSCORES', 'LIMIT', 0, limit)
end
local claimed = {}
for i = 1, #items, 2 do
  local id = items[i]
  local score = items[i + 1]
  redis.call('ZREM', KEYS[1], id)
  redis.call('ZADD', KEYS[2], score, id)
  local jk = ARGV[3] .. id
  redis.call('HSET', jk, 'status', 'running')
  redis.call('HINCRBY', jk, 'attempts', 1)
  claimed[#claimed + 1] = id
end
return claimed
";

/// Mark a job succeeded. `KEYS[1]`=run; `ARGV[1]`=job key, `ARGV[2]`=id. Returns
/// `1` if the job existed, else `0`.
const SUCCEED_SCRIPT: &str = r"
if redis.call('EXISTS', ARGV[1]) == 0 then return 0 end
redis.call('HSET', ARGV[1], 'status', 'succeeded')
redis.call('HDEL', ARGV[1], 'last_error')
redis.call('ZREM', KEYS[1], ARGV[2])
return 1
";

/// Record a failed attempt: re-queue with backoff while attempts remain, else
/// mark failed. `KEYS[1]`=due, `KEYS[2]`=run; `ARGV[1]`=job key, `ARGV[2]`=id,
/// `ARGV[3]`=now (ms), `ARGV[4]`=reason. Returns `1` if the job existed, else `0`.
const FAIL_SCRIPT: &str = r"
if redis.call('EXISTS', ARGV[1]) == 0 then return 0 end
local attempts = tonumber(redis.call('HGET', ARGV[1], 'attempts'))
local max = tonumber(redis.call('HGET', ARGV[1], 'max_attempts'))
redis.call('HSET', ARGV[1], 'last_error', ARGV[4])
redis.call('ZREM', KEYS[2], ARGV[2])
if attempts >= max then
  redis.call('HSET', ARGV[1], 'status', 'failed')
else
  local exp = math.min(attempts - 1, 12)
  local secs = math.min(2 ^ exp, 3600)
  redis.call('HSET', ARGV[1], 'status', 'queued')
  redis.call('ZADD', KEYS[1], tonumber(ARGV[3]) + secs * 1000, ARGV[2])
end
return 1
";

/// Re-queue jobs stalled in `run` past the cutoff. `KEYS[1]`=run, `KEYS[2]`=due;
/// `ARGV[1]`=cutoff (ms, exclusive), `ARGV[2]`=now (ms), `ARGV[3]`=job-key prefix.
/// Returns recovered ids.
const RECOVER_SCRIPT: &str = r"
local ids = redis.call('ZRANGEBYSCORE', KEYS[1], '-inf', '(' .. ARGV[1])
local recovered = {}
for _, id in ipairs(ids) do
  redis.call('ZREM', KEYS[1], id)
  redis.call('ZADD', KEYS[2], ARGV[2], id)
  local jk = ARGV[3] .. id
  redis.call('HSET', jk, 'status', 'queued')
  redis.call('HSET', jk, 'run_at', ARGV[2])
  recovered[#recovered + 1] = id
end
return recovered
";

/// Map a redis error to a [`PlatformError::Backend`].
fn redis_err(error: redis::RedisError) -> PlatformError {
    PlatformError::Backend { message: format!("job queue (redis): {error}") }
}

/// A durable [`JobQueue`] backed by Redis. Clone-cheap (holds a
/// [`ConnectionManager`]).
#[derive(Clone)]
pub struct RedisJobQueue {
    conn: ConnectionManager,
    clock: Arc<dyn Clock>,
    default_max_attempts: u32,
    prefix: String,
}

impl RedisJobQueue {
    /// Default key prefix.
    pub const DEFAULT_PREFIX: &'static str = "klauthed:jobs";

    /// Wrap a connection manager, using [`DEFAULT_PREFIX`](Self::DEFAULT_PREFIX)
    /// and [`DEFAULT_MAX_ATTEMPTS`]. `clock` drives all timing.
    #[must_use]
    pub fn new(conn: ConnectionManager, clock: Arc<dyn Clock>) -> Self {
        Self {
            conn,
            clock,
            default_max_attempts: DEFAULT_MAX_ATTEMPTS,
            prefix: Self::DEFAULT_PREFIX.to_owned(),
        }
    }

    /// Override the key prefix (for namespacing multiple queues on one Redis).
    #[must_use]
    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = prefix.into();
        self
    }

    /// Override the default attempt cap (clamped to at least 1).
    #[must_use]
    pub fn with_max_attempts(mut self, max_attempts: u32) -> Self {
        self.default_max_attempts = max_attempts.max(1);
        self
    }

    fn due_key(&self) -> String {
        format!("{}:due", self.prefix)
    }
    fn run_key(&self) -> String {
        format!("{}:run", self.prefix)
    }
    fn job_prefix(&self) -> String {
        format!("{}:j:", self.prefix)
    }
    fn job_key(&self, id: JobId) -> String {
        format!("{}:j:{}", self.prefix, id)
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
        let fields = [
            ("kind".to_owned(), job.kind.clone()),
            ("payload".to_owned(), payload_str),
            ("run_at".to_owned(), job.run_at.unix_millis().to_string()),
            ("attempts".to_owned(), "0".to_owned()),
            ("max_attempts".to_owned(), job.max_attempts.to_string()),
            ("status".to_owned(), "queued".to_owned()),
            ("created_at".to_owned(), job.created_at.unix_millis().to_string()),
        ];
        let mut conn = self.conn.clone();
        redis::pipe()
            .atomic()
            .hset_multiple(self.job_key(job.id), &fields)
            .ignore()
            .zadd(self.due_key(), job.id.to_string(), job.run_at.unix_millis())
            .ignore()
            .query_async::<()>(&mut conn)
            .await
            .map_err(redis_err)?;
        Ok(job)
    }

    /// Fetch the hashes for `ids` and decode them into jobs, in order.
    async fn load_jobs(&self, ids: &[String]) -> Result<Vec<EnqueuedJob>, PlatformError> {
        let mut conn = self.conn.clone();
        let mut jobs = Vec::with_capacity(ids.len());
        for id in ids {
            let key = format!("{}{}", self.job_prefix(), id);
            let hash: HashMap<String, String> = conn.hgetall(&key).await.map_err(redis_err)?;
            jobs.push(hash_to_job(id, &hash)?);
        }
        Ok(jobs)
    }
}

/// Decode a job hash (with `id`) into an [`EnqueuedJob`].
fn hash_to_job(id: &str, h: &HashMap<String, String>) -> Result<EnqueuedJob, PlatformError> {
    let field = |name: &str| -> Result<&String, PlatformError> {
        h.get(name).ok_or_else(|| PlatformError::Backend {
            message: format!("job {id} missing field '{name}'"),
        })
    };
    let num = |name: &str| -> Result<i64, PlatformError> {
        field(name)?.parse::<i64>().map_err(|e| PlatformError::Backend {
            message: format!("job {id} field '{name}' not an integer: {e}"),
        })
    };
    let id_parsed: JobId = id
        .parse()
        .map_err(|e| PlatformError::Backend { message: format!("invalid job id '{id}': {e}") })?;
    let payload: serde_json::Value = serde_json::from_str(field("payload")?).map_err(|e| {
        PlatformError::Backend { message: format!("invalid job payload json: {e}") }
    })?;
    let status = match field("status")?.as_str() {
        "queued" => JobStatus::Queued,
        "running" => JobStatus::Running,
        "succeeded" => JobStatus::Succeeded,
        "failed" => JobStatus::Failed,
        other => {
            return Err(PlatformError::Backend {
                message: format!("unknown job status '{other}'"),
            });
        }
    };
    Ok(EnqueuedJob {
        id: id_parsed,
        kind: field("kind")?.clone(),
        payload,
        run_at: Timestamp::from_unix_millis(num("run_at")?),
        attempts: u32::try_from(num("attempts")?).unwrap_or(u32::MAX),
        max_attempts: u32::try_from(num("max_attempts")?).unwrap_or(u32::MAX),
        status,
        created_at: Timestamp::from_unix_millis(num("created_at")?),
        last_error: h.get("last_error").cloned(),
    })
}

#[async_trait]
impl JobQueue for RedisJobQueue {
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
        let now = self.clock.now().unix_millis();
        let limit = limit.map_or(-1i64, |n| i64::try_from(n).unwrap_or(i64::MAX));
        let mut conn = self.conn.clone();
        let claimed: Vec<String> = redis::Script::new(CLAIM_SCRIPT)
            .key(self.due_key())
            .key(self.run_key())
            .arg(now)
            .arg(limit)
            .arg(self.job_prefix())
            .invoke_async(&mut conn)
            .await
            .map_err(redis_err)?;
        self.load_jobs(&claimed).await
    }

    async fn mark_succeeded(&self, id: JobId) -> Result<(), PlatformError> {
        let mut conn = self.conn.clone();
        let existed: i64 = redis::Script::new(SUCCEED_SCRIPT)
            .key(self.run_key())
            .arg(self.job_key(id))
            .arg(id.to_string())
            .invoke_async(&mut conn)
            .await
            .map_err(redis_err)?;
        if existed == 0 {
            return Err(PlatformError::JobNotFound { id: id.to_string() });
        }
        Ok(())
    }

    async fn mark_failed(&self, id: JobId, reason: String) -> Result<(), PlatformError> {
        let now = self.clock.now().unix_millis();
        let mut conn = self.conn.clone();
        let existed: i64 = redis::Script::new(FAIL_SCRIPT)
            .key(self.due_key())
            .key(self.run_key())
            .arg(self.job_key(id))
            .arg(id.to_string())
            .arg(now)
            .arg(reason)
            .invoke_async(&mut conn)
            .await
            .map_err(redis_err)?;
        if existed == 0 {
            return Err(PlatformError::JobNotFound { id: id.to_string() });
        }
        Ok(())
    }

    async fn dequeue_stalled(
        &self,
        stall_after: Duration,
    ) -> Result<Vec<EnqueuedJob>, PlatformError> {
        let now = self.clock.now().unix_millis();
        let stall_ms = i64::try_from(stall_after.whole_milliseconds()).unwrap_or(i64::MAX);
        let cutoff = now.saturating_sub(stall_ms);
        let mut conn = self.conn.clone();
        let recovered: Vec<String> = redis::Script::new(RECOVER_SCRIPT)
            .key(self.run_key())
            .key(self.due_key())
            .arg(cutoff)
            .arg(now)
            .arg(self.job_prefix())
            .invoke_async(&mut conn)
            .await
            .map_err(redis_err)?;
        self.load_jobs(&recovered).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use klauthed_core::time::FixedClock;

    /// Connect to a live Redis from `REDIS_URL` (default `redis://127.0.0.1/`),
    /// on a unique key prefix per test so runs don't interfere.
    async fn live_queue(clock: Arc<FixedClock>, max_attempts: u32) -> RedisJobQueue {
        let url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1/".to_owned());
        let client = redis::Client::open(url).expect("open redis client");
        let conn = ConnectionManager::new(client).await.expect("connect redis");
        RedisJobQueue::new(conn, clock)
            .with_prefix(format!("klauthed:test:{}", JobId::new()))
            .with_max_attempts(max_attempts)
    }

    #[tokio::test]
    #[ignore = "requires a live Redis at REDIS_URL"]
    async fn enqueue_claim_succeed_round_trip() {
        let clock = Arc::new(FixedClock::at_unix_millis(0));
        let queue = live_queue(clock, 3).await;

        let job = queue.enqueue("k".into(), serde_json::json!({ "a": 1 })).await.unwrap();
        assert_eq!(job.status(), JobStatus::Queued);

        let due = queue.dequeue_due(Some(10)).await.unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id(), job.id());
        assert_eq!(due[0].status(), JobStatus::Running);
        assert_eq!(due[0].attempts(), 1);
        assert_eq!(due[0].payload()["a"], 1);

        // Claimed → no longer due.
        assert!(queue.dequeue_due(Some(10)).await.unwrap().is_empty());

        queue.mark_succeeded(job.id()).await.unwrap();
        assert!(queue.dequeue_due(Some(10)).await.unwrap().is_empty());
    }

    #[tokio::test]
    #[ignore = "requires a live Redis at REDIS_URL"]
    async fn scheduled_job_waits_for_run_at() {
        let clock = Arc::new(FixedClock::at_unix_millis(0));
        let queue = live_queue(clock.clone(), 3).await;

        let run_at = clock.now().checked_add(Duration::seconds(60)).unwrap();
        let job = queue.schedule("k".into(), serde_json::json!(null), run_at).await.unwrap();
        assert!(queue.dequeue_due(None).await.unwrap().is_empty());

        clock.advance(Duration::seconds(61));
        let due = queue.dequeue_due(None).await.unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id(), job.id());
    }

    #[tokio::test]
    #[ignore = "requires a live Redis at REDIS_URL"]
    async fn mark_failed_requeues_with_backoff_then_fails_at_max() {
        let clock = Arc::new(FixedClock::at_unix_millis(0));
        let queue = live_queue(clock.clone(), 2).await;

        let job = queue.enqueue("k".into(), serde_json::json!(null)).await.unwrap();

        // Attempt 1 → re-queued with 1s backoff (not yet due).
        queue.dequeue_due(None).await.unwrap();
        queue.mark_failed(job.id(), "boom-1".into()).await.unwrap();
        assert!(queue.dequeue_due(None).await.unwrap().is_empty());

        // Attempt 2 (== max) → terminal Failed.
        clock.advance(Duration::seconds(2));
        let due = queue.dequeue_due(None).await.unwrap();
        assert_eq!(due[0].attempts(), 2);
        queue.mark_failed(job.id(), "boom-2".into()).await.unwrap();

        clock.advance(Duration::seconds(3600));
        assert!(queue.dequeue_due(None).await.unwrap().is_empty());
    }

    #[tokio::test]
    #[ignore = "requires a live Redis at REDIS_URL"]
    async fn dequeue_stalled_recovers_running_jobs() {
        let clock = Arc::new(FixedClock::at_unix_millis(0));
        let queue = live_queue(clock.clone(), 3).await;

        let job = queue.enqueue("k".into(), serde_json::json!(null)).await.unwrap();
        queue.dequeue_due(None).await.unwrap(); // -> Running, run_at = 0

        clock.advance(Duration::seconds(30));
        assert!(queue.dequeue_stalled(Duration::seconds(30)).await.unwrap().is_empty());

        clock.advance(Duration::seconds(1));
        let recovered = queue.dequeue_stalled(Duration::seconds(30)).await.unwrap();
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].id(), job.id());
        assert_eq!(recovered[0].status(), JobStatus::Queued);

        assert_eq!(queue.dequeue_due(None).await.unwrap().len(), 1);
    }
}
