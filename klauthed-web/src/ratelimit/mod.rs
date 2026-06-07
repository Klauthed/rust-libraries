//! Fixed-window rate limiting middleware with a pluggable backend.
//!
//! [`RateLimit`] is an actix [`Transform`](actix_web::dev::Transform) that caps
//! how many requests a given client may make within a rolling fixed window.
//! Clients are keyed by a configurable strategy ([`KeyBy`]) — peer IP by default,
//! or the value of a header such as `x-api-key`. When a client exceeds its budget
//! the request is rejected with `429 Too Many Requests` (via
//! [`AppError`](crate::AppError), category `RateLimited`) and a `Retry-After`
//! header indicating when the window resets.
//!
//! Counting is delegated to a [`RateLimiter`] store:
//!
//! * [`RateLimit::new`] uses an in-process [`InMemoryRateLimiter`] — each replica
//!   counts independently.
//! * [`RateLimit::with_store`] takes any `Arc<dyn RateLimiter>`, e.g. a
//!   `RedisRateLimiter`, so a fleet shares one global budget per key.
//!
//! If the backing store errors (e.g. Redis is unreachable) the middleware **fails
//! open** — it logs and lets the request through, so a limiter outage cannot take
//! the service down.
//!
//! ```no_run
//! use std::time::Duration;
//! use actix_web::App;
//! use klauthed_web::ratelimit::{KeyBy, RateLimit};
//!
//! // 100 requests per minute, keyed by the `x-api-key` header.
//! let limiter = RateLimit::new(100, Duration::from_secs(60))
//!     .key_by(KeyBy::header("x-api-key"));
//!
//! let app = App::new().wrap(limiter);
//! ```
//!
//! # Out of scope (future passes)
//!
//! Token-bucket smoothing and per-route budgets are intentionally not handled
//! here yet.

pub mod key;
pub mod middleware;

pub use key::KeyBy;
pub use middleware::{RateLimit, RateLimitService};

#[cfg(feature = "data-redis")]
#[doc(no_inline)]
pub use klauthed_data::rate_limit::RedisRateLimiter;
#[doc(no_inline)]
pub use klauthed_data::rate_limit::{InMemoryRateLimiter, RateLimitOutcome, RateLimiter};
