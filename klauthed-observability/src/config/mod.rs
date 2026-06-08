//! Typed telemetry configuration.

pub mod log;
pub mod metrics;
pub mod otel;
pub mod telemetry;

pub use log::{LogConfig, LogFormat};
pub use metrics::MetricsConfig;
pub use otel::OtelConfig;
pub use telemetry::TelemetryConfig;
