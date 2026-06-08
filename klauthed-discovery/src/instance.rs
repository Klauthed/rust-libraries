//! The [`ServiceInstance`] type.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One running instance of a service: where to reach it, plus metadata.
///
/// `service_name` is the logical discovery key (e.g. `"auth-api"`) that many
/// instances share; `instance_id` uniquely identifies this one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceInstance {
    /// Logical service name — the key looked up during resolution.
    pub service_name: String,
    /// Unique id for this instance (e.g. `host:port` or a generated id).
    pub instance_id: String,
    /// Hostname or IP address.
    pub host: String,
    /// Port the instance listens on.
    pub port: u16,
    /// Whether the instance is served over TLS (`https`).
    #[serde(default)]
    pub secure: bool,
    /// Free-form metadata (zone, version, weight, …).
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

impl ServiceInstance {
    /// A plaintext instance with no metadata. The `instance_id` defaults to
    /// `host:port`; override it with [`with_instance_id`](Self::with_instance_id).
    #[must_use]
    pub fn new(service_name: impl Into<String>, host: impl Into<String>, port: u16) -> Self {
        let host = host.into();
        Self {
            service_name: service_name.into(),
            instance_id: format!("{host}:{port}"),
            host,
            port,
            secure: false,
            metadata: BTreeMap::new(),
        }
    }

    /// Set an explicit instance id (builder form).
    #[must_use]
    pub fn with_instance_id(mut self, instance_id: impl Into<String>) -> Self {
        self.instance_id = instance_id.into();
        self
    }

    /// Mark the instance as TLS-served (builder form).
    #[must_use]
    pub fn secure(mut self, secure: bool) -> Self {
        self.secure = secure;
        self
    }

    /// Attach a metadata entry (builder form).
    #[must_use]
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// The base URL for this instance, e.g. `https://10.0.0.1:8443`.
    #[must_use]
    pub fn base_url(&self) -> String {
        let scheme = if self.secure { "https" } else { "http" };
        format!("{scheme}://{}:{}", self.host, self.port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_defaults_instance_id_to_host_port() {
        let i = ServiceInstance::new("auth-api", "10.0.0.1", 8080);
        assert_eq!(i.instance_id, "10.0.0.1:8080");
        assert_eq!(i.base_url(), "http://10.0.0.1:8080");
    }

    #[test]
    fn builders_apply() {
        let i = ServiceInstance::new("auth-api", "10.0.0.1", 8443)
            .secure(true)
            .with_instance_id("auth-1")
            .with_metadata("zone", "eu-1");
        assert_eq!(i.instance_id, "auth-1");
        assert_eq!(i.base_url(), "https://10.0.0.1:8443");
        assert_eq!(i.metadata.get("zone").map(String::as_str), Some("eu-1"));
    }

    #[test]
    fn serde_round_trips() {
        let i = ServiceInstance::new("svc", "host", 1).with_metadata("k", "v");
        let json = serde_json::to_string(&i).unwrap();
        assert_eq!(serde_json::from_str::<ServiceInstance>(&json).unwrap(), i);
    }
}
