//! Typed telemetry configuration.

use klauthed_core::config::Profile;
use serde::{Deserialize, Serialize};

/// Top-level telemetry settings for a service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryConfig {
    /// Service name reported in logs, metrics, and traces.
    #[serde(default = "default_service_name")]
    pub service_name: String,
    /// Logging / tracing-subscriber settings.
    #[serde(default)]
    pub log: LogConfig,
    /// Prometheus metrics settings.
    #[serde(default)]
    pub metrics: MetricsConfig,
    /// OpenTelemetry (OTLP) trace export settings.
    #[serde(default)]
    pub otel: OtelConfig,
}

fn default_service_name() -> String {
    "service".to_owned()
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            service_name: default_service_name(),
            log: LogConfig::default(),
            metrics: MetricsConfig::default(),
            otel: OtelConfig::default(),
        }
    }
}

impl TelemetryConfig {
    /// A config for `service_name` with profile-appropriate defaults: human
    /// `Pretty` logs on local/dev/test, structured `Json` logs on staging/prod.
    pub fn for_profile(profile: &Profile, service_name: impl Into<String>) -> Self {
        let format = if profile.requires_vault() { LogFormat::Json } else { LogFormat::Pretty };
        Self {
            service_name: service_name.into(),
            log: LogConfig { format, ..LogConfig::default() },
            ..Self::default()
        }
    }
}

/// Logging / tracing-subscriber settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogConfig {
    /// Output format.
    #[serde(default)]
    pub format: LogFormat,
    /// Filter directive (e.g. `info`, or `info,sqlx=warn`). `RUST_LOG` overrides it.
    #[serde(default = "default_level")]
    pub level: String,
    /// Whether to colorize human-readable output.
    #[serde(default = "default_true")]
    pub ansi: bool,
}

fn default_level() -> String {
    "info".to_owned()
}
fn default_true() -> bool {
    true
}

impl Default for LogConfig {
    fn default() -> Self {
        Self { format: LogFormat::default(), level: default_level(), ansi: default_true() }
    }
}

/// Log output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LogFormat {
    /// Multi-line, colorized, developer-friendly.
    #[default]
    Pretty,
    /// Single-line, terse.
    Compact,
    /// Structured JSON (one object per line) for log aggregation.
    Json,
}

/// Prometheus metrics settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetricsConfig {
    /// Whether to install the Prometheus recorder.
    #[serde(default)]
    pub enabled: bool,
}

/// OpenTelemetry (OTLP) trace export settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtelConfig {
    /// Whether to export traces via OTLP.
    #[serde(default)]
    pub enabled: bool,
    /// OTLP HTTP endpoint of the collector.
    #[serde(default = "default_otlp_endpoint")]
    pub endpoint: String,
    /// Head-sampling ratio in `[0.0, 1.0]` (1.0 = sample everything).
    #[serde(default = "default_sample_ratio")]
    pub sample_ratio: f64,
}

fn default_otlp_endpoint() -> String {
    "http://localhost:4318".to_owned()
}
fn default_sample_ratio() -> f64 {
    1.0
}

impl Default for OtelConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: default_otlp_endpoint(),
            sample_ratio: default_sample_ratio(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults_are_sensible() {
        let cfg = TelemetryConfig::default();
        assert_eq!(cfg.service_name, "service");
        assert_eq!(cfg.log.format, LogFormat::Pretty);
        assert_eq!(cfg.log.level, "info");
        assert!(!cfg.metrics.enabled);
        assert!(!cfg.otel.enabled);
        assert_eq!(cfg.otel.endpoint, "http://localhost:4318");
    }

    #[test]
    fn profile_selects_log_format() {
        assert_eq!(
            TelemetryConfig::for_profile(&Profile::Local, "svc").log.format,
            LogFormat::Pretty
        );
        assert_eq!(TelemetryConfig::for_profile(&Profile::Prod, "svc").log.format, LogFormat::Json);
    }

    #[test]
    fn deserializes_partial_config() {
        let cfg: TelemetryConfig = serde_json::from_value(json!({
            "service_name": "api",
            "log": { "format": "json", "level": "debug" },
            "otel": { "enabled": true }
        }))
        .unwrap();
        assert_eq!(cfg.service_name, "api");
        assert_eq!(cfg.log.format, LogFormat::Json);
        assert_eq!(cfg.log.level, "debug");
        assert!(cfg.otel.enabled);
        assert!(cfg.log.ansi); // default preserved
    }
}
