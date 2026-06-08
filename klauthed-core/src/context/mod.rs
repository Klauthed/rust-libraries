//! Per-request execution context.
//!
//! [`RequestContext`] carries the cross-cutting facts about the work in flight —
//! a generated request id, an inbound correlation id, tenant and principal,
//! locale, when it arrived, an optional deadline, and a free-form metadata bag.
//!
//! There are two ways to use it, by design:
//!
//! * **Explicit** (always available): construct a `RequestContext` and pass
//!   `&ctx` down the call chain. This is the source of truth — clear and testable.
//! * **Ambient** (feature `task-local`): set the context once for a request with
//!   [`RequestContext::scope`] and read it anywhere below with
//!   [`RequestContext::try_current`], without threading it through every signature.
//!
//! ```
//! use klauthed_core::context::RequestContext;
//!
//! let ctx = RequestContext::new()
//!     .with_correlation_id("trace-abc")
//!     .with_tenant("acme")
//!     .with_metadata("feature_flag", "beta");
//!
//! assert!(ctx.correlation_id().is_some());
//! assert_eq!(ctx.tenant(), Some("acme"));
//! assert_eq!(ctx.metadata_get("feature_flag"), Some("beta"));
//! ```

use crate::id::Id;

#[cfg(feature = "task-local")]
mod ambient;
pub mod request_context;

pub use request_context::RequestContext;

/// Marker tag for a request identifier.
pub struct Request;

/// The id minted for each incoming request.
pub type RequestId = Id<Request>;
