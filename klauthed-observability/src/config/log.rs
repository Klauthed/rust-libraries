//! Logging / tracing-subscriber settings: [`LogConfig`] and [`LogFormat`].

use serde::{Deserialize, Serialize};

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
