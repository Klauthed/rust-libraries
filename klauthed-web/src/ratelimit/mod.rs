//! In-memory fixed-window rate limiting middleware.
//!
//! [`RateLimit`] is an actix [`Transform`](actix_web::dev::Transform) that caps how many requests a given
//! client may make within a rolling fixed window. Clients are keyed by a
//! configurable strategy ([`KeyBy`]) — peer IP by default, or the value of a
//! header such as `x-api-key`. When a client exceeds its budget the request is
//! rejected with `429 Too Many Requests` (via [`AppError`](crate::AppError), category
//! `RateLimited`) and a `Retry-After` header indicating when the window resets.
//!
//! State is held in a `Mutex<HashMap>` shared across workers; counters reset
//! lazily when a window elapses, so memory is bounded by the number of distinct
//! active keys.
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
//! Distributed limiting (shared store), token-bucket smoothing, and per-route
//! budgets are intentionally not handled here yet.

pub mod key;
pub mod middleware;
pub(crate) mod state;

pub use key::KeyBy;
pub use middleware::{RateLimit, RateLimitService};
