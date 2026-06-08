use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::{Protocol, SpanExporter, WithExportConfig};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::{Sampler, SdkTracerProvider};
use tracing_subscriber::Layer;

use crate::config::TelemetryConfig;
use crate::error::ObservabilityError;
use crate::logging::BoxedLayer;

/// Build the OpenTelemetry tracing layer and its tracer provider.
///
/// The returned provider must be kept alive (held by the telemetry guard) and
/// shut down on exit to flush buffered spans.
pub(crate) fn trace_layer(
    config: &TelemetryConfig,
) -> Result<(BoxedLayer, SdkTracerProvider), ObservabilityError> {
    // OTLP/HTTP expects the traces signal path; derive it from the base endpoint.
    let base = config.otel.endpoint.trim_end_matches('/');
    let endpoint =
        if base.ends_with("/v1/traces") { base.to_owned() } else { format!("{base}/v1/traces") };

    let exporter = SpanExporter::builder()
        .with_http()
        .with_endpoint(endpoint)
        .with_protocol(Protocol::HttpBinary)
        .build()
        .map_err(|e| ObservabilityError::Otel(e.to_string()))?;

    let sampler = if config.otel.sample_ratio >= 1.0 {
        Sampler::AlwaysOn
    } else if config.otel.sample_ratio <= 0.0 {
        Sampler::AlwaysOff
    } else {
        Sampler::TraceIdRatioBased(config.otel.sample_ratio)
    };

    let resource = Resource::builder().with_service_name(config.service_name.clone()).build();

    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_sampler(sampler)
        .with_resource(resource)
        .build();

    // Propagate W3C `traceparent` so traces stitch across services.
    opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());

    let tracer = provider.tracer("klauthed-observability");
    let layer = tracing_opentelemetry::layer().with_tracer(tracer).boxed();

    Ok((layer, provider))
}
