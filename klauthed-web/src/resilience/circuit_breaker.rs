//! A circuit breaker that trips after repeated failures.

use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use klauthed_core::time::{Clock, Timestamp};

/// The outcome of a [`CircuitBreaker::call`] that did not return a value.
#[derive(Debug)]
pub enum CircuitError<E> {
    /// The circuit is open; the call was rejected without running the operation.
    Open,
    /// The operation ran and returned this error.
    Inner(E),
}

impl<E: fmt::Display> fmt::Display for CircuitError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open => f.write_str("circuit breaker is open"),
            Self::Inner(error) => write!(f, "{error}"),
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for CircuitError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Open => None,
            Self::Inner(error) => Some(error),
        }
    }
}

#[derive(Clone, Copy)]
enum State {
    Closed { failures: u32 },
    Open { opened_at: Timestamp },
    HalfOpen,
}

/// Fails fast once a dependency starts erroring, giving it time to recover.
///
/// After `failure_threshold` consecutive failures the breaker **opens** and
/// rejects calls with [`CircuitError::Open`] for `cooldown`. The next call then
/// runs as a **half-open** trial: success **closes** the breaker (resetting the
/// failure count); failure re-opens it. The clock is injected, so the cooldown is
/// deterministically testable with a `FixedClock`.
///
/// ```no_run
/// # async fn run() {
/// use std::sync::Arc;
/// use std::time::Duration;
/// use klauthed_core::time::SystemClock;
/// use klauthed_web::resilience::CircuitBreaker;
///
/// let breaker = CircuitBreaker::new(Arc::new(SystemClock), 5, Duration::from_secs(30));
/// let _ = breaker.call(async || reqwest_get().await).await;
/// # }
/// # async fn reqwest_get() -> Result<(), std::io::Error> { Ok(()) }
/// ```
pub struct CircuitBreaker {
    failure_threshold: u32,
    cooldown: Duration,
    clock: Arc<dyn Clock>,
    state: Mutex<State>,
}

impl CircuitBreaker {
    /// A closed breaker that opens after `failure_threshold` (≥ 1) consecutive
    /// failures and stays open for `cooldown`.
    #[must_use]
    pub fn new(clock: Arc<dyn Clock>, failure_threshold: u32, cooldown: Duration) -> Self {
        Self {
            failure_threshold: failure_threshold.max(1),
            cooldown,
            clock,
            state: Mutex::new(State::Closed { failures: 0 }),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Whether a call is currently permitted, transitioning `Open → HalfOpen`
    /// once the cooldown has elapsed.
    fn allow(&self) -> bool {
        let mut state = self.lock();
        match *state {
            State::Closed { .. } | State::HalfOpen => true,
            State::Open { opened_at } => {
                let elapsed =
                    std::time::Duration::try_from(self.clock.now().duration_since(opened_at))
                        .unwrap_or_default();
                if elapsed >= self.cooldown {
                    *state = State::HalfOpen;
                    true
                } else {
                    false
                }
            }
        }
    }

    fn record_success(&self) {
        *self.lock() = State::Closed { failures: 0 };
    }

    fn record_failure(&self) {
        let mut state = self.lock();
        let tripped = match *state {
            State::Closed { failures } => failures + 1 >= self.failure_threshold,
            State::HalfOpen => true,
            State::Open { .. } => true,
        };
        *state = if tripped {
            State::Open { opened_at: self.clock.now() }
        } else if let State::Closed { failures } = *state {
            State::Closed { failures: failures + 1 }
        } else {
            *state
        };
    }

    /// Run `op` through the breaker. Returns [`CircuitError::Open`] without
    /// running it if the circuit is open; otherwise runs it and records the
    /// outcome, returning [`CircuitError::Inner`] on failure.
    pub async fn call<T, E, F>(&self, op: F) -> Result<T, CircuitError<E>>
    where
        F: AsyncFnOnce() -> Result<T, E>,
    {
        if !self.allow() {
            return Err(CircuitError::Open);
        }
        match op().await {
            Ok(value) => {
                self.record_success();
                Ok(value)
            }
            Err(error) => {
                self.record_failure();
                Err(CircuitError::Inner(error))
            }
        }
    }

    /// Whether the breaker is currently rejecting calls (open and still cooling
    /// down). Primarily for tests and introspection.
    #[must_use]
    pub fn is_open(&self) -> bool {
        matches!(*self.lock(), State::Open { opened_at }
            if std::time::Duration::try_from(self.clock.now().duration_since(opened_at))
                .unwrap_or_default()
                < self.cooldown)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use klauthed_core::time::{Duration as CoreDuration, FixedClock};

    fn breaker(threshold: u32, cooldown_secs: u64) -> (CircuitBreaker, Arc<FixedClock>) {
        let clock = Arc::new(FixedClock::new(Timestamp::from_unix_seconds(1_000)));
        let cb = CircuitBreaker::new(clock.clone(), threshold, Duration::from_secs(cooldown_secs));
        (cb, clock)
    }

    #[tokio::test]
    async fn opens_after_threshold_consecutive_failures() {
        let (cb, _clock) = breaker(2, 30);
        assert!(cb.call(async || Err::<(), _>("e")).await.is_err());
        assert!(!cb.is_open(), "one failure is below threshold");
        assert!(cb.call(async || Err::<(), _>("e")).await.is_err());
        assert!(cb.is_open(), "second failure trips the breaker");

        // While open, calls are rejected without running the operation.
        let rejected = cb.call(async || -> Result<(), &str> { panic!("must not run") }).await;
        assert!(matches!(rejected, Err(CircuitError::Open)));
    }

    #[tokio::test]
    async fn half_opens_after_cooldown_and_closes_on_success() {
        let (cb, clock) = breaker(1, 30);
        assert!(cb.call(async || Err::<(), _>("e")).await.is_err()); // opens
        assert!(cb.is_open());

        clock.advance(CoreDuration::seconds(31)); // past cooldown → half-open
        assert!(!cb.is_open());
        let trial: Result<u32, CircuitError<&str>> = cb.call(async || Ok(7)).await;
        assert!(matches!(trial, Ok(7)), "successful trial closes the breaker");

        // Closed again: failures reset, so it takes another full threshold to trip.
        assert!(cb.call(async || Err::<(), _>("e")).await.is_err());
        assert!(cb.is_open());
    }

    #[tokio::test]
    async fn success_resets_the_failure_count() {
        let (cb, _clock) = breaker(3, 30);
        assert!(cb.call(async || Err::<(), _>("e")).await.is_err());
        assert!(cb.call(async || Err::<(), _>("e")).await.is_err());
        assert!(cb.call(async || Ok::<(), &str>(())).await.is_ok()); // resets
        // Two more failures should not trip (count restarted).
        assert!(cb.call(async || Err::<(), _>("e")).await.is_err());
        assert!(cb.call(async || Err::<(), _>("e")).await.is_err());
        assert!(!cb.is_open());
    }
}
