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
        self.windows.lock().unwrap_or_else(std::sync::PoisonError::into_inner).len()
    }

    /// Whether no keys are currently tracked.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.windows.lock().unwrap_or_else(std::sync::PoisonError::into_inner).is_empty()
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

        let mut windows = self.windows.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
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

/// One key's token-bucket state.
#[derive(Debug, Clone, Copy)]
struct Bucket {
    /// Fractional tokens currently available.
    tokens: f64,
    /// When `tokens` was last refilled.
    refilled_at: Timestamp,
}

/// A per-process **token-bucket** [`RateLimiter`].
///
/// Unlike the fixed-window [`InMemoryRateLimiter`] (which resets in hard steps),
/// the bucket refills *continuously*: it holds up to `max` tokens (the burst
/// size) and refills at `max / window` tokens per second, so traffic is smoothed
/// rather than allowed in bursts at each window boundary. Each request spends one
/// token; an empty bucket reports the time until the next token.
///
/// It implements the same [`RateLimiter`] trait with the same `(max, window)`
/// parameters, so it is a drop-in alternative wherever a fixed-window limiter is
/// used. Clock-injected for deterministic tests.
#[derive(Clone)]
pub struct InMemoryTokenBucket {
    buckets: Arc<Mutex<HashMap<String, Bucket>>>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for InMemoryTokenBucket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let len = self.buckets.lock().map(|m| m.len()).unwrap_or(0);
        f.debug_struct("InMemoryTokenBucket").field("keys", &len).finish_non_exhaustive()
    }
}

impl InMemoryTokenBucket {
    /// A token bucket driven by `clock`.
    #[must_use]
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self { buckets: Arc::new(Mutex::new(HashMap::new())), clock }
    }

    /// A token bucket driven by the real system clock.
    #[must_use]
    pub fn system() -> Self {
        Self::new(Arc::new(SystemClock))
    }
}

#[async_trait]
impl RateLimiter for InMemoryTokenBucket {
    async fn check(
        &self,
        key: &str,
        max: u32,
        window: StdDuration,
    ) -> Result<RateLimitOutcome, DataError> {
        let capacity = f64::from(max.max(1));
        // Tokens replenished per second so a full `capacity` refills over `window`.
        let refill_per_sec = capacity / window.as_secs_f64().max(f64::MIN_POSITIVE);
        let now = self.clock.now();

        let mut buckets = self.buckets.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        // New keys start full, permitting an initial burst up to `capacity`.
        let bucket =
            buckets.entry(key.to_owned()).or_insert(Bucket { tokens: capacity, refilled_at: now });

        let elapsed = now.duration_since(bucket.refilled_at).as_seconds_f64().max(0.0);
        bucket.tokens = (bucket.tokens + elapsed * refill_per_sec).min(capacity);
        bucket.refilled_at = now;

        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            Ok(RateLimitOutcome::Allowed { remaining: bucket.tokens as u32 })
        } else {
            let secs_until_token = (1.0 - bucket.tokens) / refill_per_sec;
            Ok(RateLimitOutcome::Limited {
                retry_after: StdDuration::from_secs_f64(secs_until_token),
            })
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

    fn bucket_at(millis: i64) -> (Arc<FixedClock>, InMemoryTokenBucket) {
        let clock = Arc::new(FixedClock::at_unix_millis(millis));
        (clock.clone(), InMemoryTokenBucket::new(clock))
    }

    #[tokio::test]
    async fn token_bucket_allows_initial_burst_up_to_capacity() {
        let (_clock, tb) = bucket_at(0);
        let window = StdDuration::from_secs(10); // 2 tokens / 10s
        // Starts full: two requests succeed immediately, the third is limited.
        assert!(tb.check("k", 2, window).await.unwrap().is_allowed());
        assert!(tb.check("k", 2, window).await.unwrap().is_allowed());
        assert!(!tb.check("k", 2, window).await.unwrap().is_allowed());
    }

    #[tokio::test]
    async fn token_bucket_refills_continuously() {
        let (clock, tb) = bucket_at(0);
        let window = StdDuration::from_secs(10); // refill 0.2 tokens/s
        // Drain the bucket (capacity 2).
        tb.check("k", 2, window).await.unwrap();
        tb.check("k", 2, window).await.unwrap();
        assert!(!tb.check("k", 2, window).await.unwrap().is_allowed());

        // After 5s, 0.2/s * 5s = 1 token refilled -> exactly one more allowed.
        clock.advance(Duration::seconds(5));
        assert!(tb.check("k", 2, window).await.unwrap().is_allowed());
        assert!(!tb.check("k", 2, window).await.unwrap().is_allowed());
    }

    #[tokio::test]
    async fn token_bucket_limited_reports_retry_after() {
        let (_clock, tb) = bucket_at(0);
        let window = StdDuration::from_secs(10); // 0.2 tokens/s -> ~5s for 1 token
        tb.check("k", 1, window).await.unwrap(); // capacity 1, now empty
        match tb.check("k", 1, window).await.unwrap() {
            RateLimitOutcome::Limited { retry_after } => {
                // 1 token at 0.1/s (capacity 1 over 10s) => ~10s.
                assert_eq!(retry_after.as_secs(), 10);
            }
            other => panic!("expected Limited, got {other:?}"),
        }
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
