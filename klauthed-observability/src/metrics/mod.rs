//! Prometheus metrics (feature `metrics`).
//!
//! Installs a global Prometheus recorder so the `metrics` crate's macros
//! (`counter!`, `histogram!`, …) record into it, and returns a handle that
//! renders the exposition text for a `/metrics` endpoint.

pub mod handle;
pub mod record;

pub use handle::{MetricsHandle, install};
pub use record::{inc_counter, observe, record_http_request};
