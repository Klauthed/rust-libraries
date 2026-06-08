//! Prometheus [`MetricsConfig`].

use serde::{Deserialize, Serialize};

/// Prometheus metrics settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetricsConfig {
    /// Whether to install the Prometheus recorder.
    #[serde(default)]
    pub enabled: bool,
}
