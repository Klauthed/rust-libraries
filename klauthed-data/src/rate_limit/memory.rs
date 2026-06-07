//! The in-process [`InMemoryRateLimiter`].

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration as StdDuration;

use async_trait::async_trait;
use klauthed_core::time::{Clock, Duration, SystemClock, Timestamp};

use super::{RateLimitOutcome, RateLimiter};
use crate::error::DataError;

/// One key's counter within the current fixed window.
#[derive(Debug, Clone, Copy)]
struct Window {
    started: Timestamp,
    count: u32,
}

/// A per-process, fixed-window [`RateLimiter`] backed by a `Mutex<HashMap>`.
///
/// Counters live in this process only — each replica enforces its own budget, so
/// for a multi-replica deployment with one global budget use a shared backend
/// such as [`RedisRateLimiter`](super::RedisRateLimiter). "Now" comes from an
/// injected [`Clock`], so expiry is deterministic under a `FixedClock` in tests.
/// Cloned handles share the same backing map.
#[derive(Clone)]
pub struct InMemoryRateLimiter {
    windows: Arc<Mutex<HashMap<String, Window>>>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for InMemoryRateLimiter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let len = self.windows.lock().map(|m| m.len()).unwrap_or(0);
        f.debug_struct("InMemoryRateLimiter").field("keys", &len).finish_non_exhaustive()
    }
}

impl InMemoryRateLimiter {
    /// A limiter driven by `clock`.
    #[must_use]
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self { windows: Arc::new(Mutex::new(HashMap::new())), clock }
    }

    /// A limiter driven by the real system clock.
    #[must_use]
    pub fn system() -> Self {
        Self::new(Arc::new(SystemClock))
    }

    /// Number of distinct keys currently tracked (including windows that have
    /// elapsed but not yet been overwritten).
    #[must_use]
    pub fn len(&self) -> usize {
        self.windows.lock().expect("rate-limit mutex poisoned").len()
    }

    /// Whether no keys are currently tracked.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.windows.lock().expect("rate-limit mutex poisoned").is_empty()
    }
}

#[async_trait]
impl RateLimiter for InMemoryRateLimiter {
    async fn check(
        &self,
        key: &str,
        max: u32,
        window: StdDuration,
    ) -> Result<RateLimitOutcome, DataError> {
        let max = max.max(1);
        let window_core = Duration::milliseconds(window.as_millis().min(i64::MAX as u128) as i64);
        let now = self.clock.now();

        let mut windows = self.windows.lock().expect("rate-limit mutex poisoned");
        let entry = windows.entry(key.to_owned()).or_insert(Window { started: now, count: 0 });

        // Reset the window if it has elapsed.
        if now.duration_since(entry.started) >= window_core {
            entry.started = now;
            entry.count = 0;
        }

        if entry.count >= max {
            let elapsed = now.duration_since(entry.started);
            let remaining = window_core - elapsed;
            let retry_after =
                StdDuration::from_millis(remaining.whole_milliseconds().max(0) as u64);
            Ok(RateLimitOutcome::Limited { retry_after })
        } else {
            entry.count += 1;
            Ok(RateLimitOutcome::Allowed { remaining: max - entry.count })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use klauthed_core::time::FixedClock;

    fn limiter_at(millis: i64) -> (Arc<FixedClock>, InMemoryRateLimiter) {
        let clock = Arc::new(FixedClock::at_unix_millis(millis));
        (clock.clone(), InMemoryRateLimiter::new(clock))
    }

    #[tokio::test]
    async fn allows_up_to_max_then_limits_then_resets() {
        let (clock, limiter) = limiter_at(0);
        let window = StdDuration::from_secs(10);

        assert_eq!(
            limiter.check("k", 2, window).await.unwrap(),
            RateLimitOutcome::Allowed { remaining: 1 }
        );
        assert_eq!(
            limiter.check("k", 2, window).await.unwrap(),
            RateLimitOutcome::Allowed { remaining: 0 }
        );
        assert!(!limiter.check("k", 2, window).await.unwrap().is_allowed());

        // After the window elapses the budget refreshes.
        clock.advance(Duration::seconds(10));
        assert!(limiter.check("k", 2, window).await.unwrap().is_allowed());
    }

    #[tokio::test]
    async fn keys_are_independent() {
        let (_clock, limiter) = limiter_at(0);
        let window = StdDuration::from_secs(10);
        assert!(limiter.check("a", 1, window).await.unwrap().is_allowed());
        assert!(!limiter.check("a", 1, window).await.unwrap().is_allowed());
        // A different key has its own fresh budget.
        assert!(limiter.check("b", 1, window).await.unwrap().is_allowed());
        assert_eq!(limiter.len(), 2);
    }

    #[tokio::test]
    async fn limited_reports_time_until_reset() {
        let (clock, limiter) = limiter_at(0);
        let window = StdDuration::from_secs(60);
        limiter.check("k", 1, window).await.unwrap();
        clock.advance(Duration::seconds(20));
        match limiter.check("k", 1, window).await.unwrap() {
            RateLimitOutcome::Limited { retry_after } => {
                // 60s window, 20s elapsed -> ~40s remaining.
                assert_eq!(retry_after, StdDuration::from_secs(40));
            }
            other => panic!("expected Limited, got {other:?}"),
        }
    }
}
