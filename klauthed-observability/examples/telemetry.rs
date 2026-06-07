//! Initialize telemetry from a profile and emit a few structured log events.
//!
//! Run with: `cargo run -p klauthed-observability --example telemetry`
//! (enable `--features metrics,otel` to also wire Prometheus / OTLP export).

use klauthed_core::config::Profile;
use klauthed_observability::{TelemetryConfig, init};

#[tokio::main]
async fn main() {
    let config = TelemetryConfig::for_profile(&Profile::Local, "demo-service");
    // Keep the handle alive for the program's lifetime; dropping it flushes
    // any buffered OpenTelemetry spans.
    let _telemetry = init(&config).expect("telemetry init failed");

    tracing::info!(user = "alice", "service starting");
    tracing::warn!(latency_ms = 42, "a request was slow");

    println!("emitted structured logs for service '{}'", config.service_name);
}
