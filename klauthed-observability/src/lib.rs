#![deny(unsafe_code)]
#![deny(missing_docs)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)
)]

//! Observability for klauthed services: structured logging/tracing, Prometheus
//! metrics, and OpenTelemetry trace export — from one [`TelemetryConfig`].
//!
//! [`init`] installs the global tracing subscriber (and, per feature + config,
//! the metrics recorder and the OTLP trace pipeline) and returns a [`Telemetry`]
//! handle. Keep it alive for the program's lifetime; dropping it flushes
//! OpenTelemetry spans.
//!
//! ```no_run
//! use klauthed_observability::{init, TelemetryConfig};
//! use klauthed_core::config::Profile;
//!
//! let config = TelemetryConfig::for_profile(&Profile::detect(), "billing-api");
//! let _telemetry = init(&config).expect("telemetry init");
//! tracing::info!("service starting");
//! ```
//!
//! Features:
//! * `metrics` — Prometheus recorder + a `/metrics` render handle.
//! * `otel` — OTLP trace export wired into the tracing subscriber.

mod config;
mod error;
mod logging;
mod trace;

#[cfg(feature = "metrics")]
pub mod metrics;

#[cfg(feature = "otel")]
mod otel;

pub use config::{LogConfig, LogFormat, MetricsConfig, OtelConfig, TelemetryConfig};
pub use error::ObservabilityError;
pub use trace::{RecordContext, request_span};

use tracing_subscriber::Registry;
use tracing_subscriber::prelude::*;

/// A live telemetry installation. Hold it for the program's lifetime.
pub struct Telemetry {
    // Dropped last; flushes OpenTelemetry on shutdown.
    _guard: Guard,
    #[cfg(feature = "metrics")]
    metrics: Option<metrics::MetricsHandle>,
}

impl Telemetry {
    /// The Prometheus render handle, if metrics were installed.
    #[cfg(feature = "metrics")]
    pub fn metrics(&self) -> Option<&metrics::MetricsHandle> {
        self.metrics.as_ref()
    }
}

/// Initialize telemetry from `config`, installing the global subscriber and,
/// per features and config, the metrics recorder and OTLP trace pipeline.
pub fn init(config: &TelemetryConfig) -> Result<Telemetry, ObservabilityError> {
    #[cfg(feature = "metrics")]
    let metrics = if config.metrics.enabled { Some(metrics::install()?) } else { None };

    // `mut` is only used when the otel layer is pushed below.
    #[cfg_attr(not(feature = "otel"), allow(unused_mut))]
    let mut layers: Vec<logging::BoxedLayer> = vec![logging::fmt_layer(&config.log)];

    #[cfg(feature = "otel")]
    let tracer_provider = if config.otel.enabled {
        let (layer, provider) = otel::trace_layer(config)?;
        layers.push(layer);
        Some(provider)
    } else {
        None
    };

    // Layers (typed over `Registry`) go on first; the global level filter is
    // applied outermost so it gates the whole stack.
    Registry::default()
        .with(layers)
        .with(logging::env_filter(&config.log))
        .try_init()
        .map_err(|e| ObservabilityError::Subscriber(e.to_string()))?;

    Ok(Telemetry {
        _guard: Guard {
            #[cfg(feature = "otel")]
            tracer_provider,
        },
        #[cfg(feature = "metrics")]
        metrics,
    })
}

/// Holds resources that must outlive `init` and be cleaned up on shutdown.
struct Guard {
    #[cfg(feature = "otel")]
    tracer_provider: Option<opentelemetry_sdk::trace::SdkTracerProvider>,
}

impl Drop for Guard {
    fn drop(&mut self) {
        #[cfg(feature = "otel")]
        if let Some(provider) = self.tracer_provider.take() {
            // Best-effort flush of pending spans on shutdown.
            let _ = provider.shutdown();
        }
    }
}
