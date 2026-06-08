//! The [`MetricsHandle`] and global recorder [`install`]ation.

use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

use crate::error::ObservabilityError;

/// A handle to the installed Prometheus recorder; render it for scraping.
#[derive(Clone)]
pub struct MetricsHandle {
    handle: PrometheusHandle,
}

impl MetricsHandle {
    /// Render the current metrics in Prometheus exposition format.
    ///
    /// Serve this from your `GET /metrics` route.
    pub fn render(&self) -> String {
        self.handle.render()
    }
}

/// Install the global Prometheus recorder and return its render handle.
///
/// Fails if a metrics recorder was already installed in this process.
pub fn install() -> Result<MetricsHandle, ObservabilityError> {
    let handle = PrometheusBuilder::new()
        .install_recorder()
        .map_err(|e| ObservabilityError::Metrics(e.to_string()))?;
    Ok(MetricsHandle { handle })
}
