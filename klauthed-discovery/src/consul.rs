//! [`ConsulRegistry`] — a [`ServiceRegistry`] backed by a Consul agent
//! (`feature = "consul"`).
//!
//! Registers via `PUT /v1/agent/service/register` with a TTL health check, so
//! the instance is reaped if heartbeats stop; [`heartbeat`] passes that check,
//! [`deregister`] removes the service, and [`instances`] resolves the healthy
//! instances of a service from `GET /v1/health/service/{name}?passing`.
//!
//! [`heartbeat`]: ServiceRegistry::heartbeat
//! [`deregister`]: ServiceRegistry::deregister
//! [`instances`]: ServiceRegistry::instances

use std::collections::BTreeMap;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use crate::error::DiscoveryError;
use crate::instance::ServiceInstance;
use crate::registry::ServiceRegistry;

/// Metadata key used to carry the [`ServiceInstance::secure`] flag through
/// Consul's string-only service metadata.
const SECURE_META_KEY: &str = "klauthed_secure";

/// The default TTL for the registered health check (seconds). Heartbeats must
/// arrive at least this often or Consul marks the instance unhealthy.
const DEFAULT_TTL_SECS: u64 = 30;

/// A [`ServiceRegistry`] backed by a Consul agent's HTTP API.
///
/// Point it at a reachable agent (usually the node-local agent at
/// `http://127.0.0.1:8500`). Cloneable and cheap to share behind an `Arc`.
#[derive(Debug, Clone)]
pub struct ConsulRegistry {
    base_url: String,
    client: reqwest::Client,
    ttl_secs: u64,
}

impl ConsulRegistry {
    /// Connect to the Consul agent at `agent_url` (e.g. `http://127.0.0.1:8500`),
    /// with the default 30s health-check TTL.
    #[must_use]
    pub fn new(agent_url: impl Into<String>) -> Self {
        Self {
            base_url: agent_url.into().trim_end_matches('/').to_owned(),
            client: reqwest::Client::new(),
            ttl_secs: DEFAULT_TTL_SECS,
        }
    }

    /// Use a pre-built [`reqwest::Client`] (custom timeouts, TLS, …).
    #[must_use]
    pub fn with_client(mut self, client: reqwest::Client) -> Self {
        self.client = client;
        self
    }

    /// Set the health-check TTL in seconds (builder form). Send a heartbeat at
    /// least this often.
    #[must_use]
    pub fn ttl_seconds(mut self, ttl_secs: u64) -> Self {
        self.ttl_secs = ttl_secs.max(1);
        self
    }

    /// The TTL check id Consul assigns to a service-level check.
    fn check_id(instance_id: &str) -> String {
        format!("service:{instance_id}")
    }

    /// Map a transport error to [`DiscoveryError::Backend`].
    fn transport(error: reqwest::Error) -> DiscoveryError {
        DiscoveryError::Backend(error.to_string())
    }
}

#[async_trait]
impl ServiceRegistry for ConsulRegistry {
    async fn register(&self, instance: &ServiceInstance) -> Result<(), DiscoveryError> {
        let mut meta = instance.metadata.clone();
        meta.insert(SECURE_META_KEY.to_owned(), instance.secure.to_string());

        let body = json!({
            "ID": instance.instance_id,
            "Name": instance.service_name,
            "Address": instance.host,
            "Port": instance.port,
            "Meta": meta,
            "Check": {
                "CheckID": Self::check_id(&instance.instance_id),
                "TTL": format!("{}s", self.ttl_secs),
                "DeregisterCriticalServiceAfter": "1m",
            },
        });

        let response = self
            .client
            .put(format!("{}/v1/agent/service/register", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(Self::transport)?;
        ensure_success(response, "register").await
    }

    async fn deregister(
        &self,
        _service_name: &str,
        instance_id: &str,
    ) -> Result<(), DiscoveryError> {
        let response = self
            .client
            .put(format!("{}/v1/agent/service/deregister/{instance_id}", self.base_url))
            .send()
            .await
            .map_err(Self::transport)?;
        ensure_success(response, "deregister").await
    }

    async fn heartbeat(
        &self,
        _service_name: &str,
        instance_id: &str,
    ) -> Result<(), DiscoveryError> {
        let check = Self::check_id(instance_id);
        let response = self
            .client
            .put(format!("{}/v1/agent/check/pass/{check}", self.base_url))
            .send()
            .await
            .map_err(Self::transport)?;
        ensure_success(response, "heartbeat").await
    }

    async fn instances(&self, service_name: &str) -> Result<Vec<ServiceInstance>, DiscoveryError> {
        let response = self
            .client
            .get(format!("{}/v1/health/service/{service_name}", self.base_url))
            .query(&[("passing", "true")])
            .send()
            .await
            .map_err(Self::transport)?;
        let response = ok_or_backend(response, "instances").await?;
        let entries: Vec<HealthEntry> =
            response.json().await.map_err(|e| DiscoveryError::Decode(e.to_string()))?;
        Ok(entries.into_iter().map(|entry| entry.into_instance(service_name)).collect())
    }
}

/// Treat any non-success response as a failed registry mutation.
async fn ensure_success(response: reqwest::Response, op: &str) -> Result<(), DiscoveryError> {
    let status = response.status();
    if status.is_success() {
        return Ok(());
    }
    let detail = response.text().await.unwrap_or_default();
    Err(DiscoveryError::Registration(format!("consul {op} returned {status}: {detail}")))
}

/// Treat a non-success lookup response as a backend (transient) error.
async fn ok_or_backend(
    response: reqwest::Response,
    op: &str,
) -> Result<reqwest::Response, DiscoveryError> {
    let status = response.status();
    if status.is_success() {
        Ok(response)
    } else {
        Err(DiscoveryError::Backend(format!("consul {op} returned {status}")))
    }
}

#[derive(Deserialize)]
struct HealthEntry {
    #[serde(rename = "Node")]
    node: NodeInfo,
    #[serde(rename = "Service")]
    service: ServiceInfo,
}

#[derive(Deserialize)]
struct NodeInfo {
    #[serde(rename = "Address", default)]
    address: String,
}

#[derive(Deserialize)]
struct ServiceInfo {
    #[serde(rename = "ID")]
    id: String,
    #[serde(rename = "Address", default)]
    address: String,
    #[serde(rename = "Port")]
    port: u16,
    #[serde(rename = "Meta", default)]
    meta: BTreeMap<String, String>,
}

impl HealthEntry {
    fn into_instance(self, service_name: &str) -> ServiceInstance {
        // The service may not advertise its own address; fall back to the node's.
        let host =
            if self.service.address.is_empty() { self.node.address } else { self.service.address };
        let mut metadata = self.service.meta;
        let secure = metadata.remove(SECURE_META_KEY).as_deref() == Some("true");
        ServiceInstance {
            service_name: service_name.to_owned(),
            instance_id: self.service.id,
            host,
            port: self.service.port,
            secure,
            metadata,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path, path_regex, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn register_puts_service_with_ttl_check() {
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path("/v1/agent/service/register"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let reg = ConsulRegistry::new(server.uri());
        let instance = ServiceInstance::new("auth", "10.0.0.1", 8080).with_metadata("zone", "eu-1");
        reg.register(&instance).await.expect("register");
        // `expect(1)` is asserted on drop.
    }

    #[tokio::test]
    async fn heartbeat_passes_the_service_check() {
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path("/v1/agent/check/pass/service:auth-1"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        ConsulRegistry::new(server.uri()).heartbeat("auth", "auth-1").await.expect("heartbeat");
    }

    #[tokio::test]
    async fn instances_parses_healthy_entries() {
        let server = MockServer::start().await;
        let payload = json!([
            {
                "Node": { "Address": "10.0.0.9" },
                "Service": {
                    "ID": "auth-1",
                    "Service": "auth",
                    "Address": "10.0.0.1",
                    "Port": 8443,
                    "Meta": { "klauthed_secure": "true", "zone": "eu-1" }
                }
            },
            {
                "Node": { "Address": "10.0.0.9" },
                "Service": { "ID": "auth-2", "Service": "auth", "Address": "", "Port": 8080, "Meta": {} }
            }
        ]);
        Mock::given(method("GET"))
            .and(path("/v1/health/service/auth"))
            .and(query_param("passing", "true"))
            .respond_with(ResponseTemplate::new(200).set_body_json(payload))
            .mount(&server)
            .await;

        let found = ConsulRegistry::new(server.uri()).instances("auth").await.expect("instances");
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].instance_id, "auth-1");
        assert!(found[0].secure);
        assert_eq!(found[0].base_url(), "https://10.0.0.1:8443");
        assert_eq!(found[0].metadata.get("zone").map(String::as_str), Some("eu-1"));
        // The secure marker is consumed, not surfaced as metadata.
        assert!(!found[0].metadata.contains_key("klauthed_secure"));
        // Empty service address falls back to the node address.
        assert_eq!(found[1].host, "10.0.0.9");
    }

    #[tokio::test]
    async fn non_success_register_is_registration_error() {
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path_regex(r"^/v1/agent/service/register$"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .mount(&server)
            .await;

        let err = ConsulRegistry::new(server.uri())
            .register(&ServiceInstance::new("auth", "h", 1))
            .await
            .unwrap_err();
        assert!(matches!(err, DiscoveryError::Registration(_)));
    }
}
