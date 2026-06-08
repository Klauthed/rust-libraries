//! OpenTelemetry OTLP trace export (feature `otel`).
//!
//! Builds an OTLP/HTTP span exporter, a batch tracer provider, and the
//! `tracing-opentelemetry` layer that feeds `tracing` spans into it. Also
//! installs the W3C TraceContext propagator so trace context flows across
//! service boundaries.

mod setup;

pub(crate) use setup::*;
