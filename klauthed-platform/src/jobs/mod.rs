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
//! an injected [`Clock`](klauthed_core::time::Clock), so scheduling and retry-backoff are fully testable with
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

pub mod model;
pub mod queue;

pub use model::{DEFAULT_MAX_ATTEMPTS, EnqueuedJob, Job, JobId, JobStatus};
pub use queue::{InMemoryJobQueue, JobQueue};

#[cfg(test)]
mod tests;
