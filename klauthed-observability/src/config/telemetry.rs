//! The top-level [`TelemetryConfig`].

use klauthed_core::config::Profile;
use serde::{Deserialize, Serialize};

use super::{LogConfig, LogFormat, MetricsConfig, OtelConfig};

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
