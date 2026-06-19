//! A lightweight scheduler for recurring background tasks.
//!
//! Register async tasks to run on a fixed period ([`every`](Scheduler::every)) or
//! a [`Cron`] calendar schedule ([`cron`](Scheduler::cron)),
//! [`start`](Scheduler::start) them on the current Tokio runtime, and stop them —
//! either explicitly via [`SchedulerHandle::shutdown`] or by dropping the handle.
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

use klauthed_core::time::{Clock, SystemClock};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;

pub use crate::cron::{Cron, CronError};

type TaskFuture = Pin<Box<dyn Future<Output = ()> + Send>>;
type TaskFn = Arc<dyn Fn() -> TaskFuture + Send + Sync>;

/// Builds a set of recurring tasks, then [`start`](Self::start)s them.
#[derive(Default, Clone)]
pub struct Scheduler {
    tasks: Vec<(Duration, TaskFn)>,
    cron_tasks: Vec<(Cron, TaskFn)>,
}

impl Scheduler {
    /// A scheduler with no tasks.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `task` to run every `period`. The first run happens one `period`
    /// after [`start`](Self::start) (not immediately). Runs are sequential per
    /// task; a panic in one run is isolated and the schedule continues.
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

    /// Register `task` to run on a cron `schedule` (evaluated in UTC). Runs are
    /// sequential per task; a panic in one run is isolated and the schedule
    /// continues.
    ///
    /// ```
    /// use klauthed_platform::scheduler::{Cron, Scheduler};
    /// # fn build() -> Scheduler {
    /// Scheduler::new().cron(Cron::parse("0 * * * *").unwrap(), || async {
    ///     // top of every hour
    /// })
    /// # }
    /// ```
    #[must_use]
    pub fn cron<F, Fut>(mut self, schedule: Cron, task: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let task: TaskFn = Arc::new(move || Box::pin(task()));
        self.cron_tasks.push((schedule, task));
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
        let mut handles: Vec<JoinHandle<()>> = self
            .tasks
            .into_iter()
            .map(|(period, task)| spawn_task(period, task, stop_rx.clone()))
            .collect();
        handles.extend(
            self.cron_tasks
                .into_iter()
                .map(|(schedule, task)| spawn_cron(schedule, task, stop_rx.clone())),
        );
        SchedulerHandle { _stop: stop_tx, handles }
    }
}

fn spawn_cron(schedule: Cron, task: TaskFn, mut stop: watch::Receiver<()>) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let now = SystemClock.now_datetime();
            let Some(next) = schedule.next_after(now) else {
                // An impossible schedule (e.g. Feb 30) never fires; stop quietly.
                break;
            };
            let wait = std::time::Duration::try_from(next - now).unwrap_or(Duration::ZERO);
            tokio::select! {
                _ = tokio::time::sleep(wait) => {
                    // Isolate a panicking run, like the interval scheduler.
                    let _ = tokio::spawn(task()).await;
                }
                _ = stop.changed() => break,
            }
        }
    })
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
                _ = ticker.tick() => {
                    // Run each tick in its own task and await it, so a panic in one
                    // run is isolated (surfaces as a JoinError) and the recurring
                    // schedule keeps going instead of silently dying.
                    let _ = tokio::spawn(task()).await;
                }
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
    async fn a_panicking_run_does_not_stop_the_schedule() {
        let (tx, mut rx) = mpsc::unbounded_channel::<u32>();
        let runs = std::sync::atomic::AtomicU32::new(0);
        let runs = Arc::new(runs);
        let handle = Scheduler::new()
            .every(Duration::from_secs(10), move || {
                let tx = tx.clone();
                let runs = runs.clone();
                async move {
                    let n = runs.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    let _ = tx.send(n);
                    if n == 0 {
                        panic!("boom on the first run");
                    }
                }
            })
            .start();

        tokio::time::advance(Duration::from_secs(10)).await;
        assert_eq!(rx.recv().await, Some(0), "first run executed (and panics)");

        // Despite the panic, the schedule keeps going.
        tokio::time::advance(Duration::from_secs(10)).await;
        assert_eq!(rx.recv().await, Some(1), "schedule survived the panic");

        handle.shutdown().await;
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
