//! OpenTelemetry (OTLP) trace-export settings: [`OtelConfig`].

use serde::{Deserialize, Serialize};

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
