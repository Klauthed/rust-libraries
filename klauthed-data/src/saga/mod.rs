//! Saga orchestration: run a sequence of steps, each with a compensating action,
//! and automatically roll back on failure.
//!
//! A saga models a workflow that spans several resources where a single database
//! transaction isn't possible (e.g. reserve stock, then charge a card, then
//! ship). Each [`step`](Saga::step) pairs a forward **action** with a
//! **compensation**. [`execute`](Saga::execute) runs the actions in order; if one
//! fails, the compensations for the already-completed steps run in reverse,
//! undoing the work.
//!
//! ```
//! # async fn run() -> Result<(), klauthed_data::SagaError> {
//! use klauthed_data::Saga;
//!
//! Saga::new()
//!     .step(
//!         || async { /* reserve stock */ Ok(()) },
//!         || async { /* release stock */ },
//!     )
//!     .step(
//!         || async { /* charge card  */ Ok(()) },
//!         || async { /* refund card  */ },
//!     )
//!     .execute()
//!     .await
//! # }
//! ```

use std::fmt;
use std::future::Future;
use std::pin::Pin;

use crate::error::DataError;

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;
type Action = Box<dyn FnOnce() -> BoxFuture<Result<(), DataError>> + Send>;
type Compensation = Box<dyn FnOnce() -> BoxFuture<()> + Send>;

/// A saga failed at one step; the completed steps were compensated.
#[derive(Debug)]
pub struct SagaError {
    /// The zero-based index of the step whose action failed.
    pub step: usize,
    /// The error the failing action returned.
    pub source: DataError,
}

impl fmt::Display for SagaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "saga step {} failed (completed steps compensated): {}", self.step, self.source)
    }
}

impl std::error::Error for SagaError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

/// A sequence of compensable steps, run as a unit by [`execute`](Self::execute).
#[derive(Default)]
pub struct Saga {
    steps: Vec<(Action, Compensation)>,
}

impl Saga {
    /// An empty saga.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a step: a forward `action` and the `compensation` that undoes it if
    /// a later step fails. The compensation runs only if this step's action
    /// succeeded.
    #[must_use]
    pub fn step<A, AFut, C, CFut>(mut self, action: A, compensation: C) -> Self
    where
        A: FnOnce() -> AFut + Send + 'static,
        AFut: Future<Output = Result<(), DataError>> + Send + 'static,
        C: FnOnce() -> CFut + Send + 'static,
        CFut: Future<Output = ()> + Send + 'static,
    {
        self.steps.push((
            Box::new(move || Box::pin(action())),
            Box::new(move || Box::pin(compensation())),
        ));
        self
    }

    /// Run each step's action in order. On the first failure, run the
    /// compensations for the already-completed steps in reverse order
    /// (best-effort), then return a [`SagaError`].
    ///
    /// # Errors
    /// Returns [`SagaError`] identifying the failed step if any action fails.
    pub async fn execute(self) -> Result<(), SagaError> {
        let mut completed: Vec<Compensation> = Vec::new();
        for (index, (action, compensation)) in self.steps.into_iter().enumerate() {
            match action().await {
                Ok(()) => completed.push(compensation),
                Err(source) => {
                    for compensate in completed.into_iter().rev() {
                        compensate().await;
                    }
                    return Err(SagaError { step: index, source });
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    type Log = Arc<Mutex<Vec<String>>>;

    fn log() -> Log {
        Arc::new(Mutex::new(Vec::new()))
    }
    fn push(log: &Log, entry: &str) {
        log.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(entry.to_owned());
    }
    fn entries(log: &Log) -> Vec<String> {
        log.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
    }

    /// A step whose action succeeds, recording `action`/`comp` tags when run.
    fn ok_step(saga: Saga, log: &Log, action: &'static str, comp: &'static str) -> Saga {
        let (la, lc) = (log.clone(), log.clone());
        saga.step(
            move || async move {
                push(&la, action);
                Ok(())
            },
            move || async move { push(&lc, comp) },
        )
    }

    /// A step whose action records its tag then fails; its compensation must not run.
    fn fail_step(saga: Saga, log: &Log, action: &'static str) -> Saga {
        let la = log.clone();
        saga.step(
            move || async move {
                push(&la, action);
                Err(DataError::Outbox("boom".into()))
            },
            || async { unreachable!("a failed step's compensation must not run") },
        )
    }

    #[tokio::test]
    async fn all_steps_succeed_without_compensation() {
        let l = log();
        let saga = ok_step(ok_step(Saga::new(), &l, "a1", "c1"), &l, "a2", "c2");
        assert!(saga.execute().await.is_ok());
        assert_eq!(entries(&l), ["a1", "a2"]);
    }

    #[tokio::test]
    async fn failure_compensates_completed_steps_in_reverse() {
        let l = log();
        let saga = ok_step(Saga::new(), &l, "a1", "c1");
        let saga = ok_step(saga, &l, "a2", "c2");
        let saga = fail_step(saga, &l, "a3");

        let err = saga.execute().await.unwrap_err();
        assert_eq!(err.step, 2);
        // a1, a2, a3(fails) → compensate c2 then c1 (reverse of completed).
        assert_eq!(entries(&l), ["a1", "a2", "a3", "c2", "c1"]);
    }

    #[tokio::test]
    async fn empty_saga_is_ok() {
        assert!(Saga::new().execute().await.is_ok());
    }
}
