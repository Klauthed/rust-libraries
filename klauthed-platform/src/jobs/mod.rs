//! Background jobs: a queue/store plus a simple worker.
//!
//! This module models the *queue/store* side of background work — a typed
//! [`JobId`], the [`JobStatus`] lifecycle, an [`EnqueuedJob`] record, and the
//! async [`JobQueue`] trait — and a [`JobWorker`] that drains it: claim due jobs,
//! run a [`JobHandler`], and report each outcome via
//! [`mark_succeeded`](JobQueue::mark_succeeded) / [`mark_failed`](JobQueue::mark_failed).
//! You can still drive the queue by hand instead — [`dequeue_due`](JobQueue::dequeue_due)
//! and report back yourself.
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
//! let job = queue.enqueue("send_email".into(), payload).await?;    // Queued
//! let due = queue.dequeue_due(None).await?;                         // now Running
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
//! Future work (out of scope here): a Postgres/Redis-backed [`JobQueue`],
//! dead-letter handling, and metering of job throughput.

pub mod model;
pub mod queue;
#[cfg(feature = "jobs-redis")]
pub mod redis;
#[cfg(feature = "jobs-sql")]
pub mod sql;
pub mod worker;

pub use model::{DEFAULT_MAX_ATTEMPTS, EnqueuedJob, Job, JobId, JobStatus};
pub use queue::{InMemoryJobQueue, JobQueue};
#[cfg(feature = "jobs-redis")]
pub use redis::RedisJobQueue;
#[cfg(feature = "jobs-sql")]
pub use sql::SqlJobQueue;
pub use worker::{JobHandler, JobWorker};

#[cfg(test)]
mod tests;
