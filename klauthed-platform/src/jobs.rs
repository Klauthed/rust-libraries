//! Background-job queueing abstraction (store only — no worker runtime).
//!
//! This module models the *queue/store* side of background work: a typed
//! [`JobId`], the [`JobStatus`] lifecycle, an [`EnqueuedJob`] record, and the
//! async [`JobQueue`] trait. Actually *running* a job (a worker loop, retries on
//! a real executor, etc.) is intentionally out of scope — callers
//! [`dequeue_due`](JobQueue::dequeue_due) jobs, execute them however they like,
//! and report back with [`mark_succeeded`](JobQueue::mark_succeeded) /
//! [`mark_failed`](JobQueue::mark_failed).
//!
//! [`InMemoryJobQueue`] is a thread-safe, deterministic implementation driven by
//! an injected [`Clock`], so scheduling and retry-backoff are fully testable with
//! a [`FixedClock`](klauthed_core::time::FixedClock).
//!
//! Sketch (the trait methods are `async`; see the crate tests for end-to-end use
//! with a runtime):
//!
//! ```text
//! let queue = InMemoryJobQueue::new(clock);
//! let job = queue.enqueue("send_email".into(), payload).await;     // Queued
//! let due = queue.dequeue_due(None).await;                          // now Running
//! queue.mark_succeeded(due[0].id()).await?;                         // Succeeded
//! ```
//!
//! The non-async value types are usable directly:
//!
//! ```
//! use klauthed_platform::jobs::JobStatus;
//!
//! assert!(JobStatus::Succeeded.is_terminal());
//! assert!(JobStatus::Failed.is_terminal());
//! assert!(!JobStatus::Queued.is_terminal());
//! assert!(!JobStatus::Running.is_terminal());
//! ```
//!
//! Future work (out of scope here): a Postgres/Redis-backed [`JobQueue`], a
//! worker runtime that polls [`dequeue_due`](JobQueue::dequeue_due), dead-letter
//! handling, and metering of job throughput.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;
use klauthed_core::id::Id;
use klauthed_core::time::Duration;
use klauthed_core::time::{Clock, Timestamp};
use serde::{Deserialize, Serialize};

use crate::error::PlatformError;

/// Zero-sized marker tagging a [`JobId`].
pub struct Job;

/// A typed, time-sortable background-job identifier.
pub type JobId = Id<Job>;

/// The lifecycle state of an [`EnqueuedJob`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    /// Waiting to be picked up (its `run_at` may still be in the future).
    Queued,
    /// Claimed by [`dequeue_due`](JobQueue::dequeue_due); execution in flight.
    Running,
    /// Completed successfully (terminal).
    Succeeded,
    /// Exhausted all attempts without success (terminal).
    Failed,
}

impl JobStatus {
    /// Whether this is a terminal state ([`Succeeded`](JobStatus::Succeeded) or
    /// [`Failed`](JobStatus::Failed)).
    pub fn is_terminal(self) -> bool {
        matches!(self, JobStatus::Succeeded | JobStatus::Failed)
    }
}

/// An immutable snapshot of a queued unit of background work.
///
/// Returned by [`JobQueue`] operations; callers treat it as read-only. The queue
/// owns the canonical mutable copy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnqueuedJob {
    id: JobId,
    kind: String,
    payload: serde_json::Value,
    run_at: Timestamp,
    attempts: u32,
    max_attempts: u32,
    status: JobStatus,
    created_at: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_error: Option<String>,
}

impl EnqueuedJob {
    /// The job id.
    pub fn id(&self) -> JobId {
        self.id
    }

    /// The job kind (a free-form discriminator, e.g. `"send_email"`).
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// The opaque JSON payload handed to whoever executes the job.
    pub fn payload(&self) -> &serde_json::Value {
        &self.payload
    }

    /// The earliest time the job should run.
    pub fn run_at(&self) -> Timestamp {
        self.run_at
    }

    /// How many times the job has been attempted so far.
    pub fn attempts(&self) -> u32 {
        self.attempts
    }

    /// The maximum number of attempts before the job is marked
    /// [`Failed`](JobStatus::Failed).
    pub fn max_attempts(&self) -> u32 {
        self.max_attempts
    }

    /// The current lifecycle state.
    pub fn status(&self) -> JobStatus {
        self.status
    }

    /// When the job was first enqueued.
    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }

    /// The reason recorded by the last [`mark_failed`](JobQueue::mark_failed), if
    /// any.
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }
}

/// The default cap on attempts for [`enqueue`](JobQueue::enqueue) /
/// [`schedule`](JobQueue::schedule).
pub const DEFAULT_MAX_ATTEMPTS: u32 = 5;

/// A store of background jobs.
///
/// This is the queue abstraction only: it persists jobs, hands out due ones, and
/// records terminal/retry outcomes. The trait is object-safe — implementors are
/// `Send + Sync` so a queue can be shared as `Arc<dyn JobQueue>`.
#[async_trait]
pub trait JobQueue: Send + Sync {
    /// Enqueue `kind`/`payload` to run as soon as possible (`run_at` = now).
    async fn enqueue(&self, kind: String, payload: serde_json::Value) -> EnqueuedJob;

    /// Enqueue `kind`/`payload` to run no earlier than `run_at`.
    async fn schedule(
        &self,
        kind: String,
        payload: serde_json::Value,
        run_at: Timestamp,
    ) -> EnqueuedJob;

    /// Claim up to `limit` jobs whose `run_at <= now` (and that are
    /// [`Queued`](JobStatus::Queued)), marking each [`Running`](JobStatus::Running)
    /// and bumping its attempt count. `None` means "no limit". Returned oldest
    /// (earliest `run_at`) first.
    async fn dequeue_due(&self, limit: Option<usize>) -> Vec<EnqueuedJob>;

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
    async fn dequeue_stalled(&self, stall_after: Duration) -> Vec<EnqueuedJob>;
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
        self.jobs.lock().expect("jobs lock poisoned").get(&id).cloned()
    }

    /// The number of jobs currently held (in any state).
    pub fn len(&self) -> usize {
        self.jobs.lock().expect("jobs lock poisoned").len()
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
        self.jobs.lock().expect("jobs lock poisoned").insert(id, job.clone());
        job
    }
}

#[async_trait]
impl JobQueue for InMemoryJobQueue {
    async fn enqueue(&self, kind: String, payload: serde_json::Value) -> EnqueuedJob {
        let now = self.clock.now();
        self.insert(kind, payload, now)
    }

    async fn schedule(
        &self,
        kind: String,
        payload: serde_json::Value,
        run_at: Timestamp,
    ) -> EnqueuedJob {
        self.insert(kind, payload, run_at)
    }

    async fn dequeue_due(&self, limit: Option<usize>) -> Vec<EnqueuedJob> {
        let now = self.clock.now();
        let mut guard = self.jobs.lock().expect("jobs lock poisoned");

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

        due.into_iter()
            .map(|(_, id)| {
                let job = guard.get_mut(&id).expect("job present");
                job.status = JobStatus::Running;
                job.attempts += 1;
                job.clone()
            })
            .collect()
    }

    async fn mark_succeeded(&self, id: JobId) -> Result<(), PlatformError> {
        let mut guard = self.jobs.lock().expect("jobs lock poisoned");
        let job =
            guard.get_mut(&id).ok_or_else(|| PlatformError::JobNotFound { id: id.to_string() })?;
        job.status = JobStatus::Succeeded;
        job.last_error = None;
        Ok(())
    }

    async fn mark_failed(&self, id: JobId, reason: String) -> Result<(), PlatformError> {
        let now = self.clock.now();
        let mut guard = self.jobs.lock().expect("jobs lock poisoned");
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

    async fn dequeue_stalled(&self, stall_after: Duration) -> Vec<EnqueuedJob> {
        let now = self.clock.now();
        let mut guard = self.jobs.lock().expect("jobs lock poisoned");

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
        recovered
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use klauthed_core::time::FixedClock;

    fn queue(max_attempts: u32) -> (Arc<FixedClock>, InMemoryJobQueue) {
        let clock = Arc::new(FixedClock::at_unix_millis(0));
        let q = InMemoryJobQueue::with_max_attempts(clock.clone(), max_attempts);
        (clock, q)
    }

    #[tokio::test]
    async fn enqueue_then_dequeue_due_marks_running() {
        let (_clock, q) = queue(5);
        let job = q.enqueue("k".into(), serde_json::json!({"a": 1})).await;
        assert_eq!(job.status(), JobStatus::Queued);
        assert_eq!(job.attempts(), 0);

        let due = q.dequeue_due(None).await;
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id(), job.id());
        assert_eq!(due[0].status(), JobStatus::Running);
        assert_eq!(due[0].attempts(), 1);

        // No longer queued, so a second poll returns nothing.
        assert!(q.dequeue_due(None).await.is_empty());
    }

    #[tokio::test]
    async fn future_job_is_not_due_until_clock_advances() {
        let (clock, q) = queue(5);
        let now = clock.now();
        let run_at = now.checked_add(Duration::seconds(60)).unwrap();
        let job = q.schedule("k".into(), serde_json::json!(null), run_at).await;

        // Not due yet.
        assert!(q.dequeue_due(None).await.is_empty());
        assert_eq!(q.get(job.id()).unwrap().status(), JobStatus::Queued);

        // Advance past run_at.
        clock.advance(Duration::seconds(61));
        let due = q.dequeue_due(None).await;
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id(), job.id());
    }

    #[tokio::test]
    async fn dequeue_due_respects_limit_and_ordering() {
        let (clock, q) = queue(5);
        let base = clock.now();
        // Three jobs with increasing run_at, all already due.
        clock.set(base.checked_add(Duration::seconds(100)).unwrap());
        let a = q.schedule("k".into(), serde_json::json!("a"), base).await;
        let b = q
            .schedule(
                "k".into(),
                serde_json::json!("b"),
                base.checked_add(Duration::seconds(1)).unwrap(),
            )
            .await;
        let _c = q
            .schedule(
                "k".into(),
                serde_json::json!("c"),
                base.checked_add(Duration::seconds(2)).unwrap(),
            )
            .await;

        let due = q.dequeue_due(Some(2)).await;
        assert_eq!(due.len(), 2);
        assert_eq!(due[0].id(), a.id());
        assert_eq!(due[1].id(), b.id());
    }

    #[tokio::test]
    async fn mark_failed_requeues_with_backoff_until_max_then_stays_failed() {
        let (clock, q) = queue(3);
        let job = q.enqueue("k".into(), serde_json::json!(null)).await;

        // Attempt 1.
        let due = q.dequeue_due(None).await;
        assert_eq!(due[0].attempts(), 1);
        q.mark_failed(job.id(), "boom-1".into()).await.unwrap();
        let after1 = q.get(job.id()).unwrap();
        assert_eq!(after1.status(), JobStatus::Queued);
        assert_eq!(after1.last_error(), Some("boom-1"));
        // Backoff after 1 attempt = 1s.
        assert_eq!(after1.run_at().duration_since(clock.now()).whole_seconds(), 1);

        // Advance and run attempt 2.
        clock.advance(Duration::seconds(2));
        let due = q.dequeue_due(None).await;
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].attempts(), 2);
        q.mark_failed(job.id(), "boom-2".into()).await.unwrap();
        let after2 = q.get(job.id()).unwrap();
        assert_eq!(after2.status(), JobStatus::Queued);
        // Backoff after 2 attempts = 2s.
        assert_eq!(after2.run_at().duration_since(clock.now()).whole_seconds(), 2);

        // Advance and run attempt 3 (== max_attempts).
        clock.advance(Duration::seconds(3));
        let due = q.dequeue_due(None).await;
        assert_eq!(due[0].attempts(), 3);
        q.mark_failed(job.id(), "boom-3".into()).await.unwrap();
        let after3 = q.get(job.id()).unwrap();
        assert_eq!(after3.status(), JobStatus::Failed);
        assert!(after3.status().is_terminal());
        assert_eq!(after3.last_error(), Some("boom-3"));

        // A failed job is never due again.
        clock.advance(Duration::seconds(3600));
        assert!(q.dequeue_due(None).await.is_empty());
    }

    #[tokio::test]
    async fn mark_succeeded_is_terminal_and_clears_error() {
        let (_clock, q) = queue(5);
        let job = q.enqueue("k".into(), serde_json::json!(null)).await;
        q.dequeue_due(None).await;
        q.mark_failed(job.id(), "transient".into()).await.unwrap();
        q.dequeue_due(None).await; // not due (backoff) — but force success anyway
        q.mark_succeeded(job.id()).await.unwrap();
        let done = q.get(job.id()).unwrap();
        assert_eq!(done.status(), JobStatus::Succeeded);
        assert_eq!(done.last_error(), None);
    }

    #[tokio::test]
    async fn mark_unknown_job_is_not_found() {
        let (_clock, q) = queue(5);
        let err = q.mark_succeeded(JobId::new()).await.unwrap_err();
        assert!(matches!(err, PlatformError::JobNotFound { .. }));
    }

    // ── dequeue_stalled tests ─────────────────────────────────────────────────

    #[tokio::test]
    async fn fresh_queued_job_never_stalls() {
        let (_clock, q) = queue(5);
        q.enqueue("k".into(), serde_json::json!(null)).await;
        // Still Queued — not Running — so it must not appear in stall recovery.
        let recovered = q.dequeue_stalled(Duration::ZERO).await;
        assert!(recovered.is_empty());
    }

    #[tokio::test]
    async fn running_job_within_stall_window_is_not_recovered() {
        let (clock, q) = queue(5);
        let job = q.enqueue("k".into(), serde_json::json!(null)).await;
        // Dequeue: status -> Running, run_at stays at t=0.
        q.dequeue_due(None).await;

        // Advance by exactly stall_after (not *strictly* greater).
        let stall_after = Duration::seconds(30);
        clock.advance(stall_after);

        let recovered = q.dequeue_stalled(stall_after).await;
        assert!(recovered.is_empty(), "job still within window must not be recovered");
        assert_eq!(q.get(job.id()).unwrap().status(), JobStatus::Running);
    }

    #[tokio::test]
    async fn running_job_past_stall_window_is_recovered_to_queued() {
        let (clock, q) = queue(5);
        let job = q.enqueue("k".into(), serde_json::json!(null)).await;
        // run_at = t=0; after dequeue the job is Running at run_at=t=0.
        q.dequeue_due(None).await;

        let stall_after = Duration::seconds(30);
        // Advance past the stall window.
        clock.advance(Duration::seconds(31));

        let recovered = q.dequeue_stalled(stall_after).await;
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].id(), job.id());
        assert_eq!(recovered[0].status(), JobStatus::Queued);
        // run_at is reset to now (immediately due).
        assert_eq!(recovered[0].run_at(), clock.now());
    }

    #[tokio::test]
    async fn recovered_jobs_appear_in_next_dequeue_due() {
        let (clock, q) = queue(5);
        let job = q.enqueue("k".into(), serde_json::json!(null)).await;
        q.dequeue_due(None).await;
        // Simulate a stall.
        clock.advance(Duration::seconds(61));
        let recovered = q.dequeue_stalled(Duration::seconds(60)).await;
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].id(), job.id());

        // The recovered job must now be picked up by dequeue_due.
        let due = q.dequeue_due(None).await;
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id(), job.id());
        assert_eq!(due[0].status(), JobStatus::Running);
    }

    #[tokio::test]
    async fn succeeded_job_is_not_recovered_by_dequeue_stalled() {
        let (clock, q) = queue(5);
        let job = q.enqueue("k".into(), serde_json::json!(null)).await;
        q.dequeue_due(None).await;
        q.mark_succeeded(job.id()).await.unwrap();

        // Even past the stall window, Succeeded jobs must be ignored.
        clock.advance(Duration::seconds(999));
        let recovered = q.dequeue_stalled(Duration::ZERO).await;
        assert!(recovered.is_empty());
    }

    #[test]
    fn enqueued_job_round_trips_through_json() {
        let job = EnqueuedJob {
            id: JobId::new(),
            kind: "k".into(),
            payload: serde_json::json!({"x": 1}),
            run_at: Timestamp::from_unix_millis(0),
            attempts: 0,
            max_attempts: 5,
            status: JobStatus::Queued,
            created_at: Timestamp::from_unix_millis(0),
            last_error: None,
        };
        let json = serde_json::to_string(&job).unwrap();
        let back: EnqueuedJob = serde_json::from_str(&json).unwrap();
        assert_eq!(job, back);
    }
}
