//! `klauthed-data::rate_limit`: the `RateLimiter` trait with fixed-window and
//! token-bucket in-memory backends (clock-injected for determinism).

use std::sync::Arc;
use std::time::Duration;

use klauthed_core::time::{Duration as CoreDuration, FixedClock};
use klauthed_data::rate_limit::{
    InMemoryRateLimiter, InMemoryTokenBucket, RateLimitOutcome, RateLimiter,
};

pub async fn run() {
    let clock = Arc::new(FixedClock::at_unix_millis(0));
    let window = Duration::from_secs(60);

    // Fixed-window: 2 requests per 60s; the 3rd in the same window is limited.
    let fixed = InMemoryRateLimiter::new(clock.clone());
    let mut allowed = 0;
    for _ in 0..3 {
        if fixed.check("ip:1.2.3.4", 2, window).await.unwrap().is_allowed() {
            allowed += 1;
        }
    }
    println!("  fixed-window (2/60s): {allowed} of 3 requests allowed");
    assert_eq!(allowed, 2);

    // Token-bucket: capacity 2, refills continuously at 2/60s.
    let bucket = InMemoryTokenBucket::new(clock.clone());
    bucket.check("ip", 2, window).await.unwrap();
    bucket.check("ip", 2, window).await.unwrap();
    assert!(!bucket.check("ip", 2, window).await.unwrap().is_allowed()); // drained
    clock.advance(CoreDuration::seconds(30)); // ~1 token refilled
    let after = bucket.check("ip", 2, window).await.unwrap();
    assert!(matches!(after, RateLimitOutcome::Allowed { .. }));
    println!("  token-bucket: drained, then one token refilled after 30s");
}
