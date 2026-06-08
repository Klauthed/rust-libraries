//! HTTP server configuration (`ServerConfig`).

use serde::{Deserialize, Serialize};

/// HTTP server binding and runtime settings (e.g. for an actix-web service).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Bind address (default `0.0.0.0`).
    #[serde(default = "default_host")]
    pub host: String,
    /// Bind port (default `8080`).
    #[serde(default = "default_port")]
    pub port: u16,
    /// Worker thread count; `None` lets the server pick (usually CPU count).
    #[serde(default)]
    pub workers: Option<usize>,
    /// Per-request timeout in seconds.
    #[serde(default = "default_request_timeout_secs")]
    pub request_timeout_secs: u64,
    /// Whether the server terminates TLS itself.
    #[serde(default)]
    pub tls: bool,
}

fn default_host() -> String {
    "0.0.0.0".to_owned()
}
fn default_port() -> u16 {
    8080
}
fn default_request_timeout_secs() -> u64 {
    30
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            workers: None,
            request_timeout_secs: default_request_timeout_secs(),
            tls: false,
        }
    }
}

impl ServerConfig {
    /// The `host:port` string suitable for binding a listener.
    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults_and_bind_address() {
        let cfg: ServerConfig = serde_json::from_value(json!({})).unwrap();
        assert_eq!(cfg.bind_address(), "0.0.0.0:8080");
        assert_eq!(cfg.request_timeout_secs, 30);
    }

    #[test]
    fn overrides_apply() {
        let cfg: ServerConfig =
            serde_json::from_value(json!({ "host": "127.0.0.1", "port": 9000 })).unwrap();
        assert_eq!(cfg.bind_address(), "127.0.0.1:9000");
    }
}
