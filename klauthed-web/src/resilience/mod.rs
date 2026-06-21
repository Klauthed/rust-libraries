//! Resilience patterns for fallible async operations (e.g. outbound calls):
//! [`RetryPolicy`] — retry with exponential backoff — and [`CircuitBreaker`] —
//! fail fast after a dependency starts erroring, then probe for recovery.

pub mod circuit_breaker;
pub mod retry;

pub use circuit_breaker::{CircuitBreaker, CircuitError};
pub use retry::RetryPolicy;
