# klauthed-observability

Observability for klauthed services: structured logging/tracing, Prometheus metrics, and
OpenTelemetry trace export — all from one `TelemetryConfig`.

`init` installs the global tracing subscriber (and, per feature + config, the metrics
recorder and the OTLP trace pipeline) and returns a `Telemetry` handle. Keep it alive for
the program's lifetime; dropping it flushes OpenTelemetry spans.

```rust,no_run
use klauthed_observability::{init, TelemetryConfig};
use klauthed_core::config::Profile;

let config = TelemetryConfig::for_profile(&Profile::detect(), "billing-api");
let _telemetry = init(&config).expect("telemetry init");
tracing::info!("service starting");
```

## Features

| Feature | Enables |
|---------|---------|
| `metrics` | Prometheus recorder + a `/metrics` render handle |
| `otel` | OTLP trace export wired into the tracing subscriber |

---

Part of the [klauthed rust-libraries](../README.md) workspace.
Browse the API: `cargo doc -p klauthed-observability --open`.

## License

Dual-licensed under [MIT](../LICENSE-MIT) or [Apache-2.0](../LICENSE-APACHE), at your option.
