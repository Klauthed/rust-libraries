//! A lightweight interval scheduler for recurring background tasks.
//!
//! Register async tasks to run on a fixed period, [`start`](Scheduler::start)
//! them on the current Tokio runtime, and stop them — either explicitly via
//! [`SchedulerHandle::shutdown`] or by dropping the handle.
//!
//! ```no_run
//! use std::time::Duration;
//! use klauthed_platform::scheduler::Scheduler;
//!
//! # async fn run() {
//! let handle = Scheduler::new()
//!     .every(Duration::from_secs(60), || async {
//!         // … do periodic work …
//!     })
//!     .start();
//!
//! // … later, on shutdown:
//! handle.shutdown().await;
//! # }
//! ```
//!
//! This runs tasks in-process; for distributed or persistent scheduling, drive a
//! [`JobQueue`](crate::jobs::JobQueue) from a task registered here.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;

type TaskFuture = Pin<Box<dyn Future<Output = ()> + Send>>;
type TaskFn = Arc<dyn Fn() -> TaskFuture + Send + Sync>;

/// Builds a set of recurring tasks, then [`start`](Self::start)s them.
#[derive(Default, Clone)]
pub struct Scheduler {
    tasks: Vec<(Duration, TaskFn)>,
}

impl Scheduler {
    /// A scheduler with no tasks.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `task` to run every `period`. The first run happens one `period`
    /// after [`start`](Self::start) (not immediately).
    #[must_use]
    pub fn every<F, Fut>(mut self, period: Duration, task: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let task: TaskFn = Arc::new(move || Box::pin(task()));
        self.tasks.push((period, task));
        self
    }

    /// Spawn every registered task on the current Tokio runtime, returning a
    /// [`SchedulerHandle`] that stops them when shut down or dropped.
    ///
    /// # Panics
    /// Panics if called outside a Tokio runtime (like any [`tokio::spawn`]).
    #[must_use]
    pub fn start(self) -> SchedulerHandle {
        let (stop_tx, stop_rx) = watch::channel(());
        let handles = self
            .tasks
            .into_iter()
            .map(|(period, task)| spawn_task(period, task, stop_rx.clone()))
            .collect();
        SchedulerHandle { _stop: stop_tx, handles }
    }
}

fn spawn_task(period: Duration, task: TaskFn, mut stop: watch::Receiver<()>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(period);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        // The first tick of a Tokio interval is immediate; consume it so the
        // first run is one full period after start.
        ticker.tick().await;
        loop {
            tokio::select! {
                _ = ticker.tick() => task().await,
                // Resolves when the handle signals stop or is dropped (sender gone).
                _ = stop.changed() => break,
            }
        }
    })
}

/// Controls the tasks spawned by [`Scheduler::start`]. Dropping the handle stops
/// the tasks; [`shutdown`](Self::shutdown) does so and waits for them to finish.
pub struct SchedulerHandle {
    // Dropping the sender makes every task's `stop.changed()` resolve, so tasks
    // stop even if the handle is dropped without an explicit shutdown.
    _stop: watch::Sender<()>,
    handles: Vec<JoinHandle<()>>,
}

impl SchedulerHandle {
    /// Signal all tasks to stop and wait for each to finish its current run.
    pub async fn shutdown(self) {
        // Dropping the sender signals the tasks; then await their completion.
        let Self { _stop, handles } = self;
        drop(_stop);
        for handle in handles {
            let _ = handle.await;
        }
    }

    /// The number of running tasks.
    #[must_use]
    pub fn task_count(&self) -> usize {
        self.handles.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[tokio::test(start_paused = true)]
    async fn runs_each_period_then_stops_on_shutdown() {
        let (tx, mut rx) = mpsc::unbounded_channel::<()>();
        let handle = Scheduler::new()
            .every(Duration::from_secs(10), move || {
                let tx = tx.clone();
                async move {
                    let _ = tx.send(());
                }
            })
            .start();
        assert_eq!(handle.task_count(), 1);

        // Nothing before the first period elapses.
        assert!(rx.try_recv().is_err());

        tokio::time::advance(Duration::from_secs(10)).await;
        rx.recv().await.expect("first run after one period");

        tokio::time::advance(Duration::from_secs(10)).await;
        rx.recv().await.expect("second run after another period");

        handle.shutdown().await;

        // No further runs after shutdown, however far time advances.
        tokio::time::advance(Duration::from_secs(120)).await;
        tokio::task::yield_now().await;
        assert!(rx.try_recv().is_err(), "no runs after shutdown");
    }

    #[tokio::test(start_paused = true)]
    async fn dropping_the_handle_stops_tasks() {
        let (tx, mut rx) = mpsc::unbounded_channel::<()>();
        let handle = Scheduler::new()
            .every(Duration::from_secs(5), move || {
                let tx = tx.clone();
                async move {
                    let _ = tx.send(());
                }
            })
            .start();

        tokio::time::advance(Duration::from_secs(5)).await;
        rx.recv().await.expect("one run");

        drop(handle);
        tokio::time::advance(Duration::from_secs(60)).await;
        tokio::task::yield_now().await;
        assert!(rx.try_recv().is_err(), "dropping the handle stops the task");
    }
}
