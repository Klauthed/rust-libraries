//! The background-job model: [`Job`]/[`JobId`], [`JobStatus`], and the
//! [`EnqueuedJob`] record.

use klauthed_core::id::Id;
use klauthed_core::time::Timestamp;
use serde::{Deserialize, Serialize};

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
    /// Claimed by [`dequeue_due`](super::JobQueue::dequeue_due); execution in flight.
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
/// Returned by [`JobQueue`](super::JobQueue) operations; callers treat it as read-only. The queue
/// owns the canonical mutable copy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnqueuedJob {
    pub(crate) id: JobId,
    pub(crate) kind: String,
    pub(crate) payload: serde_json::Value,
    pub(crate) run_at: Timestamp,
    pub(crate) attempts: u32,
    pub(crate) max_attempts: u32,
    pub(crate) status: JobStatus,
    pub(crate) created_at: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) last_error: Option<String>,
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

    /// The reason recorded by the last [`mark_failed`](super::JobQueue::mark_failed), if
    /// any.
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }
}

/// The default cap on attempts for [`enqueue`](super::JobQueue::enqueue) /
/// [`schedule`](super::JobQueue::schedule).
pub const DEFAULT_MAX_ATTEMPTS: u32 = 5;
