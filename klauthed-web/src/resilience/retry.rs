//! Retry with exponential backoff.

use std::time::Duration;

/// How a fallible async operation is retried: a bounded attempt count with
/// exponential backoff (capped) between tries.
#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    max_attempts: u32,
    base: Duration,
    max: Duration,
    multiplier: f64,
}

impl RetryPolicy {
    /// Default: up to 3 attempts, 100 ms base backoff doubling to a 10 s cap.
    #[must_use]
    pub fn new() -> Self {
        Self {
            max_attempts: 3,
            base: Duration::from_millis(100),
            max: Duration::from_secs(10),
            multiplier: 2.0,
        }
    }

    /// Maximum number of attempts (clamped to at least 1).
    #[must_use]
    pub fn max_attempts(mut self, attempts: u32) -> Self {
        self.max_attempts = attempts.max(1);
        self
    }

    /// Backoff before the first retry; subsequent retries scale by the multiplier.
    #[must_use]
    pub fn base_backoff(mut self, base: Duration) -> Self {
        self.base = base;
        self
    }

    /// Upper bound on any single backoff delay.
    #[must_use]
    pub fn max_backoff(mut self, max: Duration) -> Self {
        self.max = max;
        self
    }

    /// Per-retry backoff growth factor (e.g. `2.0` doubles each time).
    #[must_use]
    pub fn multiplier(mut self, multiplier: f64) -> Self {
        self.multiplier = multiplier;
        self
    }

    /// The backoff delay before the retry following `attempt` (1-based), capped at
    /// [`max_backoff`](Self::max_backoff).
    fn backoff(&self, attempt: u32) -> Duration {
        let scaled = self.base.as_secs_f64() * self.multiplier.powi((attempt - 1) as i32);
        let capped = scaled.min(self.max.as_secs_f64());
        Duration::try_from_secs_f64(capped).unwrap_or(self.max)
    }

    /// Run `op`, retrying on `Err` up to `max_attempts`, sleeping the backoff
    /// between tries. Returns the value on success, or the last error if every
    /// attempt fails.
    ///
    /// Must run on a Tokio runtime (the backoff uses [`tokio::time::sleep`]).
    pub async fn retry<T, E, F>(&self, mut op: F) -> Result<T, E>
    where
        F: AsyncFnMut() -> Result<T, E>,
    {
        let mut attempt = 1;
        loop {
            match op().await {
                Ok(value) => return Ok(value),
                Err(error) => {
                    if attempt >= self.max_attempts {
                        return Err(error);
                    }
                    tokio::time::sleep(self.backoff(attempt)).await;
                    attempt += 1;
                }
            }
        }
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[tokio::test(start_paused = true)]
    async fn succeeds_after_transient_failures() {
        let attempts = Arc::new(AtomicU32::new(0));
        let counter = attempts.clone();

        let result: Result<u32, &str> = RetryPolicy::new()
            .max_attempts(5)
            .retry(async || {
                let n = counter.fetch_add(1, Ordering::SeqCst) + 1;
                if n < 3 { Err("transient") } else { Ok(n) }
            })
            .await;

        assert_eq!(result, Ok(3));
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn gives_up_after_max_attempts() {
        let attempts = Arc::new(AtomicU32::new(0));
        let counter = attempts.clone();

        let result: Result<(), &str> = RetryPolicy::new()
            .max_attempts(3)
            .retry(async || {
                counter.fetch_add(1, Ordering::SeqCst);
                Err("always")
            })
            .await;

        assert_eq!(result, Err("always"));
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn first_attempt_success_does_not_retry() {
        let attempts = Arc::new(AtomicU32::new(0));
        let counter = attempts.clone();

        let result: Result<&str, ()> = RetryPolicy::new()
            .retry(async || {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok("ok")
            })
            .await;

        assert_eq!(result, Ok("ok"));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn backoff_grows_then_caps() {
        let policy = RetryPolicy::new()
            .base_backoff(Duration::from_secs(1))
            .multiplier(2.0)
            .max_backoff(Duration::from_secs(5));
        assert_eq!(policy.backoff(1), Duration::from_secs(1));
        assert_eq!(policy.backoff(2), Duration::from_secs(2));
        assert_eq!(policy.backoff(3), Duration::from_secs(4));
        assert_eq!(policy.backoff(4), Duration::from_secs(5)); // capped
    }
}
