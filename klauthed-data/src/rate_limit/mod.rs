//! Fixed-window rate limiting with a pluggable backend.
//!
//! A [`RateLimiter`] records requests against a string key and reports whether
//! each is [`Allowed`](RateLimitOutcome::Allowed) or
//! [`Limited`](RateLimitOutcome::Limited) within a rolling fixed window of
//! `max` requests per `window`.
//!
//! Two backends are provided:
//!
//! * [`InMemoryRateLimiter`] — a clock-injected `Mutex<HashMap>`, per-process
//!   (each replica counts independently). Ideal for single-node deployments and
//!   tests (drive it with a `FixedClock`).
//! * [`RedisRateLimiter`] (`redis` feature) — a shared counter in Redis, so a
//!   fleet of replicas enforces one global budget per key.
//!
//! ```
//! use std::sync::Arc;
//! use std::time::Duration;
//! use klauthed_core::time::FixedClock;
//! use klauthed_data::rate_limit::{InMemoryRateLimiter, RateLimiter, RateLimitOutcome};
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), klauthed_data::DataError> {
//! let clock = Arc::new(FixedClock::at_unix_millis(0));
//! let limiter = InMemoryRateLimiter::new(clock.clone());
//! let window = Duration::from_secs(60);
//!
//! // First request of two is allowed.
//! assert!(matches!(limiter.check("ip:1.2.3.4", 2, window).await?, RateLimitOutcome::Allowed { .. }));
//! limiter.check("ip:1.2.3.4", 2, window).await?; // second
//! // Third exceeds the budget.
//! assert!(matches!(limiter.check("ip:1.2.3.4", 2, window).await?, RateLimitOutcome::Limited { .. }));
//! # Ok(())
//! # }
//! ```

use std::time::Duration;

use async_trait::async_trait;

use crate::error::DataError;

pub mod memory;
#[cfg(feature = "redis")]
pub mod redis;

pub use memory::InMemoryRateLimiter;
#[cfg(feature = "redis")]
pub use redis::RedisRateLimiter;

/// The result of recording one request against a key in its current window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitOutcome {
    /// The request is within budget. `remaining` is how many more are allowed
    /// before the window resets.
    Allowed {
        /// Requests still permitted in the current window.
        remaining: u32,
    },
    /// The budget is exhausted; the caller should retry after `retry_after`.
    Limited {
        /// Time until the current window resets.
        retry_after: Duration,
    },
}

impl RateLimitOutcome {
    /// Whether the request was allowed.
    #[must_use]
    pub fn is_allowed(&self) -> bool {
        matches!(self, RateLimitOutcome::Allowed { .. })
    }
}

/// A fixed-window rate limiter keyed by an arbitrary string.
///
/// Implementations are `Send + Sync` so a limiter can be shared as
/// `Arc<dyn RateLimiter>` across worker threads.
#[async_trait]
pub trait RateLimiter: Send + Sync {
    /// Record one request for `key`, permitting up to `max` per `window`.
    ///
    /// `max` is clamped to at least 1. Returns the [`RateLimitOutcome`].
    ///
    /// # Errors
    /// Returns [`DataError`] only on a backend failure (e.g. a Redis error); an
    /// over-budget request is a successful [`Limited`](RateLimitOutcome::Limited)
    /// outcome, not an error.
    async fn check(
        &self,
        key: &str,
        max: u32,
        window: Duration,
    ) -> Result<RateLimitOutcome, DataError>;
}
