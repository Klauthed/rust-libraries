//! [`EurekaRegistry`] — a [`ServiceRegistry`] backed by a Netflix Eureka server
//! (`feature = "eureka"`).
//!
//! Registers via `POST /eureka/apps/{app}`, renews the lease with
//! `PUT /eureka/apps/{app}/{id}` ([`heartbeat`]), removes via `DELETE`, and
//! resolves `UP` instances from `GET /eureka/apps/{app}`. Eureka's JSON has two
//! quirks this module hides: ports are `{ "$": 8080, "@enabled": "true" }`
//! objects, and the `instance` field is a bare object when a single instance is
//! registered but an array when several are.
//!
//! [`heartbeat`]: ServiceRegistry::heartbeat

use std::collections::BTreeMap;

use async_trait::async_trait;
use serde::{Deserialize, Deserializer};
use serde_json::json;

use crate::error::DiscoveryError;
use crate::instance::ServiceInstance;
use crate::registry::ServiceRegistry;

/// A [`ServiceRegistry`] backed by a Eureka server's REST API.
///
/// Point it at the Eureka base URL (e.g. `http://eureka:8761`). Cloneable and
/// cheap to share behind an `Arc`.
#[derive(Debug, Clone)]
pub struct EurekaRegistry {
    base_url: String,
    client: reqwest::Client,
}

impl EurekaRegistry {
    /// Connect to the Eureka server at `base_url` (e.g. `http://eureka:8761`).
    #[must_use]
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_owned(),
            client: reqwest::Client::new(),
        }
    }

    /// Use a pre-built [`reqwest::Client`] (custom timeouts, TLS, …).
    #[must_use]
    pub fn with_client(mut self, client: reqwest::Client) -> Self {
        self.client = client;
        self
    }

    fn transport(error: reqwest::Error) -> DiscoveryError {
        DiscoveryError::Backend(error.to_string())
    }
}

#[async_trait]
impl ServiceRegistry for EurekaRegistry {
    async fn register(&self, instance: &ServiceInstance) -> Result<(), DiscoveryError> {
        // Eureka models a plaintext and a TLS port separately; advertise whichever
        // matches this instance's scheme and disable the other.
        let (plain, secure) = if instance.secure { ("false", "true") } else { ("true", "false") };
        let body = json!({
            "instance": {
                "instanceId": instance.instance_id,
                "hostName": instance.host,
                "app": instance.service_name,
                "ipAddr": instance.host,
                "vipAddress": instance.service_name,
                "secureVipAddress": instance.service_name,
                "status": "UP",
                "port": { "$": instance.port, "@enabled": plain },
                "securePort": { "$": instance.port, "@enabled": secure },
                "dataCenterInfo": {
                    "@class": "com.netflix.appinfo.InstanceInfo$DefaultDataCenterInfo",
                    "name": "MyOwn"
                },
                "metadata": instance.metadata,
            }
        });

        let response = self
            .client
            .post(format!("{}/eureka/apps/{}", self.base_url, instance.service_name))
            .json(&body)
            .send()
            .await
            .map_err(Self::transport)?;
        ensure_success(response, "register").await
    }

    async fn deregister(
        &self,
        service_name: &str,
        instance_id: &str,
    ) -> Result<(), DiscoveryError> {
        let response = self
            .client
            .delete(format!("{}/eureka/apps/{service_name}/{instance_id}", self.base_url))
            .send()
            .await
            .map_err(Self::transport)?;
        ensure_success(response, "deregister").await
    }

    async fn heartbeat(&self, service_name: &str, instance_id: &str) -> Result<(), DiscoveryError> {
        let response = self
            .client
            .put(format!("{}/eureka/apps/{service_name}/{instance_id}", self.base_url))
            .send()
            .await
            .map_err(Self::transport)?;
        ensure_success(response, "heartbeat").await
    }

    async fn instances(&self, service_name: &str) -> Result<Vec<ServiceInstance>, DiscoveryError> {
        let response = self
            .client
            .get(format!("{}/eureka/apps/{service_name}", self.base_url))
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(Self::transport)?;

        // An unknown application is "no instances", not an error.
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(Vec::new());
        }
        if !response.status().is_success() {
            return Err(DiscoveryError::Backend(format!(
                "eureka instances returned {}",
                response.status()
            )));
        }

        let parsed: AppsResponse =
            response.json().await.map_err(|e| DiscoveryError::Decode(e.to_string()))?;
        Ok(parsed
            .application
            .instance
            .into_iter()
            .filter(|i| i.status.eq_ignore_ascii_case("UP"))
            .map(|i| i.into_instance(service_name))
            .collect())
    }
}

async fn ensure_success(response: reqwest::Response, op: &str) -> Result<(), DiscoveryError> {
    let status = response.status();
    if status.is_success() {
        Ok(())
    } else {
        Err(DiscoveryError::Registration(format!("eureka {op} returned {status}")))
    }
}

#[derive(Deserialize)]
struct AppsResponse {
    application: Application,
}

#[derive(Deserialize)]
struct Application {
    #[serde(default, deserialize_with = "one_or_many")]
    instance: Vec<EurekaInstance>,
}

#[derive(Deserialize)]
struct EurekaInstance {
    #[serde(rename = "instanceId", default)]
    instance_id: String,
    #[serde(rename = "hostName", default)]
    host_name: String,
    #[serde(rename = "ipAddr", default)]
    ip_addr: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    port: PortInfo,
    #[serde(rename = "securePort", default)]
    secure_port: PortInfo,
    #[serde(default)]
    metadata: BTreeMap<String, String>,
}

#[derive(Deserialize, Default)]
struct PortInfo {
    #[serde(rename = "$", default)]
    value: u16,
    #[serde(rename = "@enabled", default)]
    enabled: String,
}

impl PortInfo {
    fn is_enabled(&self) -> bool {
        self.enabled.eq_ignore_ascii_case("true")
    }
}

impl EurekaInstance {
    fn into_instance(self, service_name: &str) -> ServiceInstance {
        let secure = self.secure_port.is_enabled();
        let port = if secure { self.secure_port.value } else { self.port.value };
        let host = if self.ip_addr.is_empty() { self.host_name } else { self.ip_addr };
        let instance_id =
            if self.instance_id.is_empty() { format!("{host}:{port}") } else { self.instance_id };
        ServiceInstance {
            service_name: service_name.to_owned(),
            instance_id,
            host,
            port,
            secure,
            metadata: self.metadata,
        }
    }
}

/// Eureka renders the `instance` field as a bare object for one instance and an
/// array for several; accept both.
fn one_or_many<'de, D>(deserializer: D) -> Result<Vec<EurekaInstance>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(Box<EurekaInstance>),
        Many(Vec<EurekaInstance>),
    }
    Ok(match OneOrMany::deserialize(deserializer)? {
        OneOrMany::One(instance) => vec![*instance],
        OneOrMany::Many(instances) => instances,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn register_posts_instance() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/eureka/apps/auth"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;

        let reg = EurekaRegistry::new(server.uri());
        reg.register(&ServiceInstance::new("auth", "10.0.0.1", 8080)).await.expect("register");
    }

    #[tokio::test]
    async fn heartbeat_and_deregister() {
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path("/eureka/apps/auth/auth-1"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        Mock::given(method("DELETE"))
            .and(path("/eureka/apps/auth/auth-1"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let reg = EurekaRegistry::new(server.uri());
        reg.heartbeat("auth", "auth-1").await.expect("heartbeat");
        reg.deregister("auth", "auth-1").await.expect("deregister");
    }

    #[tokio::test]
    async fn instances_parses_array_and_port_objects() {
        let server = MockServer::start().await;
        let payload = json!({
            "application": {
                "name": "AUTH",
                "instance": [
                    {
                        "instanceId": "auth-1",
                        "hostName": "host1",
                        "ipAddr": "10.0.0.1",
                        "status": "UP",
                        "port": { "$": 8080, "@enabled": "false" },
                        "securePort": { "$": 8443, "@enabled": "true" },
                        "metadata": { "zone": "eu-1" }
                    },
                    {
                        "instanceId": "auth-2",
                        "hostName": "host2",
                        "ipAddr": "10.0.0.2",
                        "status": "DOWN",
                        "port": { "$": 8080, "@enabled": "true" },
                        "securePort": { "$": 0, "@enabled": "false" }
                    }
                ]
            }
        });
        Mock::given(method("GET"))
            .and(path("/eureka/apps/auth"))
            .respond_with(ResponseTemplate::new(200).set_body_json(payload))
            .mount(&server)
            .await;

        let found = EurekaRegistry::new(server.uri()).instances("auth").await.expect("instances");
        // The DOWN instance is filtered out.
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].instance_id, "auth-1");
        assert!(found[0].secure);
        assert_eq!(found[0].base_url(), "https://10.0.0.1:8443");
        assert_eq!(found[0].metadata.get("zone").map(String::as_str), Some("eu-1"));
    }

    #[tokio::test]
    async fn instances_accepts_single_object_form() {
        let server = MockServer::start().await;
        let payload = json!({
            "application": {
                "name": "AUTH",
                "instance": {
                    "instanceId": "solo",
                    "ipAddr": "10.0.0.9",
                    "status": "UP",
                    "port": { "$": 9000, "@enabled": "true" },
                    "securePort": { "$": 0, "@enabled": "false" }
                }
            }
        });
        Mock::given(method("GET"))
            .and(path("/eureka/apps/auth"))
            .respond_with(ResponseTemplate::new(200).set_body_json(payload))
            .mount(&server)
            .await;

        let found = EurekaRegistry::new(server.uri()).instances("auth").await.expect("instances");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].base_url(), "http://10.0.0.9:9000");
    }

    #[tokio::test]
    async fn unknown_app_is_empty_not_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/eureka/apps/missing"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let found =
            EurekaRegistry::new(server.uri()).instances("missing").await.expect("instances");
        assert!(found.is_empty());
    }
}
