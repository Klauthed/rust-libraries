//! Context-aware tracing spans.
//!
//! Bridges a [`RequestContext`] into the `tracing` world: [`request_span`]
//! mints a `request` span carrying the cross-cutting identifiers
//! (`request_id`, and, when present, `correlation_id`, `tenant`, `principal`,
//! `locale`), and [`RecordContext::record_context`] attaches those same fields
//! to an existing span.
//!
//! Entering the returned span makes its fields appear on every child span and
//! event emitted while it is active. With the JSON subscriber installed by
//! [`crate::init`] this means each log line is automatically tagged with the
//! request's identity — no manual threading required.
//!
//! Future work: automatic span creation from a tower/actix layer, and OTLP
//! exemplars correlating these spans with metrics, belong in the consuming
//! crate (e.g. `klauthed-web`), not here.
//!
//! ```
//! use klauthed_core::context::RequestContext;
//! use klauthed_observability::request_span;
//!
//! let ctx = RequestContext::new().with_tenant("acme");
//! let span = request_span(&ctx);
//! let _entered = span.enter();
//! tracing::info!("handling request"); // carries request_id + tenant
//! ```

#[cfg(feature = "otel")]
pub mod propagation;
pub mod record;
pub mod span;

pub use record::RecordContext;
pub use span::request_span;
