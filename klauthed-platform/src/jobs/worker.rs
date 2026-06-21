//! A simple worker that drains a [`JobQueue`].

use std::sync::Arc;

use async_trait::async_trait;

use super::{EnqueuedJob, JobQueue};
use crate::error::PlatformError;

/// Processes one dequeued [`EnqueuedJob`].
///
/// Return `Ok(())` to mark the job succeeded; return `Err(reason)` to mark it
/// failed — the queue then applies retry/backoff (or moves it to `Failed` once
/// attempts are exhausted), storing `reason` as the job's `last_error`.
#[async_trait]
pub trait JobHandler: Send + Sync {
    /// Handle one job.
    async fn handle(&self, job: &EnqueuedJob) -> Result<(), String>;
}

/// Drains a [`JobQueue`]: claim due jobs, run the [`JobHandler`] for each, and
/// record the outcome. [`run_once`](Self::run_once) does one batch — call it
/// periodically (e.g. from the platform `scheduler`, behind its feature) to make
/// a long-running worker.
pub struct JobWorker {
    queue: Arc<dyn JobQueue>,
    handler: Arc<dyn JobHandler>,
    batch_size: usize,
}

impl JobWorker {
    /// A worker draining up to 100 jobs per [`run_once`](Self::run_once).
    #[must_use]
    pub fn new(queue: Arc<dyn JobQueue>, handler: Arc<dyn JobHandler>) -> Self {
        Self { queue, handler, batch_size: 100 }
    }

    /// Set the maximum number of jobs claimed per pass (clamped to at least 1).
    #[must_use]
    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = batch_size.max(1);
        self
    }

    /// Claim up to `batch_size` due jobs, run the handler for each, and mark each
    /// succeeded or failed. Returns the number of jobs processed.
    ///
    /// # Errors
    /// Returns [`PlatformError`] if recording an outcome against the queue fails.
    pub async fn run_once(&self) -> Result<usize, PlatformError> {
        let jobs = self.queue.dequeue_due(Some(self.batch_size)).await;
        let processed = jobs.len();
        for job in &jobs {
            match self.handler.handle(job).await {
                Ok(()) => self.queue.mark_succeeded(job.id()).await?,
                Err(reason) => self.queue.mark_failed(job.id(), reason).await?,
            }
        }
        Ok(processed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::{InMemoryJobQueue, JobStatus};
    use klauthed_core::time::SystemClock;
    use std::sync::Mutex;

    struct Recorder {
        handled: Mutex<Vec<String>>,
        fail: bool,
    }
    impl Recorder {
        fn new(fail: bool) -> Arc<Self> {
            Arc::new(Self { handled: Mutex::new(Vec::new()), fail })
        }
        fn handled(&self) -> Vec<String> {
            self.handled.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
        }
    }
    #[async_trait]
    impl JobHandler for Recorder {
        async fn handle(&self, job: &EnqueuedJob) -> Result<(), String> {
            self.handled
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(job.kind().to_owned());
            if self.fail { Err("boom".into()) } else { Ok(()) }
        }
    }

    #[tokio::test]
    async fn processes_due_jobs_and_marks_them_succeeded() {
        let queue = Arc::new(InMemoryJobQueue::new(Arc::new(SystemClock)));
        let j1 = queue.enqueue("email".into(), serde_json::json!({})).await;
        let j2 = queue.enqueue("sms".into(), serde_json::json!({})).await;
        let handler = Recorder::new(false);

        let processed = JobWorker::new(queue.clone(), handler.clone()).run_once().await.unwrap();

        assert_eq!(processed, 2);
        assert_eq!(handler.handled().len(), 2);
        assert_eq!(queue.get(j1.id()).unwrap().status(), JobStatus::Succeeded);
        assert_eq!(queue.get(j2.id()).unwrap().status(), JobStatus::Succeeded);
    }

    #[tokio::test]
    async fn failing_handler_marks_the_job_failed_with_its_reason() {
        let queue = Arc::new(InMemoryJobQueue::with_max_attempts(Arc::new(SystemClock), 1));
        let job = queue.enqueue("email".into(), serde_json::json!({})).await;

        let processed =
            JobWorker::new(queue.clone(), Recorder::new(true)).run_once().await.unwrap();

        assert_eq!(processed, 1);
        let stored = queue.get(job.id()).unwrap();
        assert_eq!(stored.status(), JobStatus::Failed);
        assert_eq!(stored.last_error(), Some("boom"));
    }

    #[tokio::test]
    async fn empty_queue_processes_nothing() {
        let queue = Arc::new(InMemoryJobQueue::new(Arc::new(SystemClock)));
        let processed = JobWorker::new(queue, Recorder::new(false)).run_once().await.unwrap();
        assert_eq!(processed, 0);
    }
}
