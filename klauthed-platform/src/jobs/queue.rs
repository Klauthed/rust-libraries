//! The [`JobQueue`] store trait and the clock-driven [`InMemoryJobQueue`].

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use klauthed_core::time::{Clock, Duration, Timestamp};

use crate::error::PlatformError;

use super::{DEFAULT_MAX_ATTEMPTS, EnqueuedJob, JobId, JobStatus};

/// A store of background jobs.
///
/// This is the queue abstraction only: it persists jobs, hands out due ones, and
/// records terminal/retry outcomes. The trait is object-safe — implementors are
/// `Send + Sync` so a queue can be shared as `Arc<dyn JobQueue>`.
#[async_trait]
pub trait JobQueue: Send + Sync {
    /// Enqueue `kind`/`payload` to run as soon as possible (`run_at` = now).
    ///
    /// # Errors
    /// Returns [`PlatformError`] if a durable backend fails to persist the job.
    async fn enqueue(
        &self,
        kind: String,
        payload: serde_json::Value,
    ) -> Result<EnqueuedJob, PlatformError>;

    /// Enqueue `kind`/`payload` to run no earlier than `run_at`.
    ///
    /// # Errors
    /// Returns [`PlatformError`] if a durable backend fails to persist the job.
    async fn schedule(
        &self,
        kind: String,
        payload: serde_json::Value,
        run_at: Timestamp,
    ) -> Result<EnqueuedJob, PlatformError>;

    /// Claim up to `limit` jobs whose `run_at <= now` (and that are
    /// [`Queued`](JobStatus::Queued)), marking each [`Running`](JobStatus::Running)
    /// and bumping its attempt count. `None` means "no limit". Returned oldest
    /// (earliest `run_at`) first.
    ///
    /// # Errors
    /// Returns [`PlatformError`] if a durable backend fails to claim jobs.
    async fn dequeue_due(&self, limit: Option<usize>) -> Result<Vec<EnqueuedJob>, PlatformError>;

    /// Mark a running job [`Succeeded`](JobStatus::Succeeded).
    async fn mark_succeeded(&self, id: JobId) -> Result<(), PlatformError>;

    /// Record a failed attempt. If attempts remain, the job is re-queued with an
    /// exponential backoff applied to `run_at`; once `attempts >= max_attempts`
    /// it transitions to [`Failed`](JobStatus::Failed). `reason` is stored as the
    /// job's [`last_error`](EnqueuedJob::last_error).
    async fn mark_failed(&self, id: JobId, reason: String) -> Result<(), PlatformError>;

    /// Re-queue jobs that have been in the [`Running`](JobStatus::Running) state
    /// for longer than `stall_after`. Returns the re-queued jobs (now `Queued`
    /// with a fresh `run_at`).
    ///
    /// A job is considered stalled when `now - run_at > stall_after` AND its
    /// status is `Running`. Re-queued jobs get `run_at = now` (immediately due)
    /// so they are picked up by the next `dequeue_due` call.
    ///
    /// Calling this periodically (e.g. from a health-check loop or a cron) is
    /// the recommended pattern for detecting and recovering from crashed workers.
    ///
    /// # Errors
    /// Returns [`PlatformError`] if a durable backend fails to recover jobs.
    async fn dequeue_stalled(
        &self,
        stall_after: Duration,
    ) -> Result<Vec<EnqueuedJob>, PlatformError>;
}

/// Exponential backoff for the `n`-th attempt (1-based): `base * 2^(n-1)`,
/// capped, in seconds. Used by [`InMemoryJobQueue::mark_failed`].
fn backoff_for_attempt(attempts: u32) -> Duration {
    // base = 1s, doubling, capped at ~1 hour to keep run_at sane.
    let exp = attempts.saturating_sub(1).min(12);
    let secs = 1i64.checked_shl(exp).unwrap_or(i64::MAX).min(3600);
    Duration::seconds(secs)
}

/// A thread-safe, in-memory [`JobQueue`] driven by an injected [`Clock`].
///
/// Suitable for tests and single-process use. All timing decisions (due-ness,
/// retry backoff) read the injected clock, so a
/// [`FixedClock`](klauthed_core::time::FixedClock) makes behavior deterministic.
pub struct InMemoryJobQueue {
    clock: Arc<dyn Clock>,
    default_max_attempts: u32,
    // Insertion order is irrelevant; due-ness is decided by run_at + clock.
    jobs: Mutex<HashMap<JobId, EnqueuedJob>>,
}

impl InMemoryJobQueue {
    /// A new, empty queue using `clock` for all time decisions and
    /// [`DEFAULT_MAX_ATTEMPTS`] for newly enqueued jobs.
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self::with_max_attempts(clock, DEFAULT_MAX_ATTEMPTS)
    }

    /// As [`new`](Self::new), but overriding the default attempt cap.
    pub fn with_max_attempts(clock: Arc<dyn Clock>, default_max_attempts: u32) -> Self {
        Self {
            clock,
            default_max_attempts: default_max_attempts.max(1),
            jobs: Mutex::new(HashMap::new()),
        }
    }

    /// A snapshot of the job with `id`, if present.
    pub fn get(&self, id: JobId) -> Option<EnqueuedJob> {
        self.jobs.lock().unwrap_or_else(std::sync::PoisonError::into_inner).get(&id).cloned()
    }

    /// The number of jobs currently held (in any state).
    pub fn len(&self) -> usize {
        self.jobs.lock().unwrap_or_else(std::sync::PoisonError::into_inner).len()
    }

    /// Whether the queue holds no jobs.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn insert(&self, kind: String, payload: serde_json::Value, run_at: Timestamp) -> EnqueuedJob {
        let now = self.clock.now();
        let id = JobId::new();
        let job = EnqueuedJob {
            id,
            kind,
            payload,
            run_at,
            attempts: 0,
            max_attempts: self.default_max_attempts,
            status: JobStatus::Queued,
            created_at: now,
            last_error: None,
        };
        self.jobs.lock().unwrap_or_else(std::sync::PoisonError::into_inner).insert(id, job.clone());
        job
    }
}

#[async_trait]
impl JobQueue for InMemoryJobQueue {
    async fn enqueue(
        &self,
        kind: String,
        payload: serde_json::Value,
    ) -> Result<EnqueuedJob, PlatformError> {
        let now = self.clock.now();
        Ok(self.insert(kind, payload, now))
    }

    async fn schedule(
        &self,
        kind: String,
        payload: serde_json::Value,
        run_at: Timestamp,
    ) -> Result<EnqueuedJob, PlatformError> {
        Ok(self.insert(kind, payload, run_at))
    }

    async fn dequeue_due(&self, limit: Option<usize>) -> Result<Vec<EnqueuedJob>, PlatformError> {
        let now = self.clock.now();
        let mut guard = self.jobs.lock().unwrap_or_else(std::sync::PoisonError::into_inner);

        // Collect ids of due, queued jobs, oldest run_at first (id breaks ties
        // deterministically).
        let mut due: Vec<(Timestamp, JobId)> = guard
            .values()
            .filter(|j| j.status == JobStatus::Queued && j.run_at <= now)
            .map(|j| (j.run_at, j.id))
            .collect();
        due.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

        if let Some(limit) = limit {
            due.truncate(limit);
        }

        Ok(due
            .into_iter()
            .map(|(_, id)| {
                #[allow(clippy::expect_used, reason = "id was just collected from this same guard")]
                let job = guard.get_mut(&id).expect("job present");
                job.status = JobStatus::Running;
                job.attempts += 1;
                job.clone()
            })
            .collect())
    }

    async fn mark_succeeded(&self, id: JobId) -> Result<(), PlatformError> {
        let mut guard = self.jobs.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let job =
            guard.get_mut(&id).ok_or_else(|| PlatformError::JobNotFound { id: id.to_string() })?;
        job.status = JobStatus::Succeeded;
        job.last_error = None;
        Ok(())
    }

    async fn mark_failed(&self, id: JobId, reason: String) -> Result<(), PlatformError> {
        let now = self.clock.now();
        let mut guard = self.jobs.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let job =
            guard.get_mut(&id).ok_or_else(|| PlatformError::JobNotFound { id: id.to_string() })?;

        job.last_error = Some(reason);

        if job.attempts >= job.max_attempts {
            job.status = JobStatus::Failed;
        } else {
            // Re-queue with backoff based on the number of attempts made so far.
            let delay = backoff_for_attempt(job.attempts);
            job.run_at = now.checked_add(delay).unwrap_or(now);
            job.status = JobStatus::Queued;
        }
        Ok(())
    }

    async fn dequeue_stalled(
        &self,
        stall_after: Duration,
    ) -> Result<Vec<EnqueuedJob>, PlatformError> {
        let now = self.clock.now();
        let mut guard = self.jobs.lock().unwrap_or_else(std::sync::PoisonError::into_inner);

        let mut recovered = Vec::new();
        for job in guard.values_mut() {
            if job.status != JobStatus::Running {
                continue;
            }
            // Stalled when now - run_at > stall_after.
            if now.duration_since(job.run_at) > stall_after {
                job.status = JobStatus::Queued;
                job.run_at = now;
                recovered.push(job.clone());
            }
        }
        Ok(recovered)
    }
}
