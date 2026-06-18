//! [`KubernetesRegistry`] — a read-only [`ServiceRegistry`] over the Kubernetes
//! Endpoints API (`feature = "kubernetes"`).
//!
//! In Kubernetes the platform owns the service lifecycle: pods are "registered"
//! by being scheduled, and the control plane maintains an `Endpoints` object per
//! `Service` listing the ready pod addresses. So this backend implements only
//! discovery — [`instances`] resolves a service's ready endpoints from
//! `GET /api/v1/namespaces/{ns}/endpoints/{service}` — while [`register`],
//! [`deregister`], and [`heartbeat`] return an error (there is nothing to push).
//!
//! Build it with [`in_cluster`](KubernetesRegistry::in_cluster) inside a pod (it
//! reads the mounted service-account token, CA, and namespace), or
//! [`new`](KubernetesRegistry::new) against an explicit API base URL (e.g. a
//! `kubectl proxy`) for local use and tests.
//!
//! [`instances`]: ServiceRegistry::instances
//! [`register`]: ServiceRegistry::register
//! [`deregister`]: ServiceRegistry::deregister
//! [`heartbeat`]: ServiceRegistry::heartbeat

use async_trait::async_trait;
use serde::Deserialize;

use crate::error::DiscoveryError;
use crate::instance::ServiceInstance;
use crate::registry::ServiceRegistry;

/// Where the kubelet mounts the pod's service-account credentials.
const SA_DIR: &str = "/var/run/secrets/kubernetes.io/serviceaccount";

/// A read-only [`ServiceRegistry`] backed by the Kubernetes Endpoints API.
///
/// Cloneable and cheap to share behind an `Arc`.
#[derive(Debug, Clone)]
pub struct KubernetesRegistry {
    base_url: String,
    namespace: String,
    token: Option<String>,
    port_name: Option<String>,
    client: reqwest::Client,
}

impl KubernetesRegistry {
    /// Target the API server at `api_base_url` (e.g. `http://127.0.0.1:8001` from
    /// `kubectl proxy`), namespace `default`, no auth. Use
    /// [`in_cluster`](Self::in_cluster) for production.
    #[must_use]
    pub fn new(api_base_url: impl Into<String>) -> Self {
        Self {
            base_url: api_base_url.into().trim_end_matches('/').to_owned(),
            namespace: "default".to_owned(),
            token: None,
            port_name: None,
            client: reqwest::Client::new(),
        }
    }

    /// Configure from the in-pod service account: the API server from
    /// `KUBERNETES_SERVICE_HOST`/`KUBERNETES_SERVICE_PORT`, and the token, CA, and
    /// namespace mounted under `/var/run/secrets/kubernetes.io/serviceaccount`.
    ///
    /// # Errors
    /// Returns [`DiscoveryError::Backend`] if the environment or mounted files are
    /// missing or the TLS client cannot be built (i.e. not running in a pod).
    pub fn in_cluster() -> Result<Self, DiscoveryError> {
        let read = |what: &str, path: String| {
            std::fs::read(&path)
                .map_err(|e| DiscoveryError::Backend(format!("kubernetes {what} ({path}): {e}")))
        };
        let read_str = |what: &str, path: String| {
            String::from_utf8(read(what, path)?)
                .map_err(|e| DiscoveryError::Backend(format!("kubernetes {what}: {e}")))
        };

        let host = std::env::var("KUBERNETES_SERVICE_HOST")
            .map_err(|_| DiscoveryError::Backend("KUBERNETES_SERVICE_HOST not set".to_owned()))?;
        let port = std::env::var("KUBERNETES_SERVICE_PORT")
            .map_err(|_| DiscoveryError::Backend("KUBERNETES_SERVICE_PORT not set".to_owned()))?;
        let token = read_str("token", format!("{SA_DIR}/token"))?;
        let namespace = read_str("namespace", format!("{SA_DIR}/namespace"))?.trim().to_owned();
        let ca = read("ca.crt", format!("{SA_DIR}/ca.crt"))?;

        let certificate = reqwest::Certificate::from_pem(&ca)
            .map_err(|e| DiscoveryError::Backend(format!("kubernetes ca.crt: {e}")))?;
        let client = reqwest::Client::builder()
            .add_root_certificate(certificate)
            .build()
            .map_err(|e| DiscoveryError::Backend(format!("kubernetes TLS client: {e}")))?;

        Ok(Self {
            base_url: format!("https://{host}:{port}"),
            namespace,
            token: Some(token),
            port_name: None,
            client,
        })
    }

    /// Look services up in `namespace` (builder form; default `default`).
    #[must_use]
    pub fn namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = namespace.into();
        self
    }

    /// Authenticate with this bearer `token` (builder form).
    #[must_use]
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    /// Select the named service port from each endpoint subset (builder form).
    /// Without one, the subset's first port is used.
    #[must_use]
    pub fn port_name(mut self, name: impl Into<String>) -> Self {
        self.port_name = Some(name.into());
        self
    }

    /// Use a pre-built [`reqwest::Client`] (custom timeouts, TLS, …).
    #[must_use]
    pub fn with_client(mut self, client: reqwest::Client) -> Self {
        self.client = client;
        self
    }

    /// k8s manages registration; the mutating operations are unsupported.
    fn read_only(op: &str) -> DiscoveryError {
        DiscoveryError::Registration(format!(
            "kubernetes discovery is read-only; cannot {op} (the platform manages pod lifecycle)"
        ))
    }

    /// Pick the port to advertise for a subset.
    fn select_port<'a>(&self, ports: &'a [EndpointPort]) -> Option<&'a EndpointPort> {
        match &self.port_name {
            Some(name) => ports.iter().find(|p| p.name.as_deref() == Some(name.as_str())),
            None => ports.first(),
        }
    }

    /// Map an `Endpoints` object's ready addresses to [`ServiceInstance`]s.
    fn map_endpoints(&self, service_name: &str, endpoints: &Endpoints) -> Vec<ServiceInstance> {
        let mut instances = Vec::new();
        for subset in &endpoints.subsets {
            let Some(port) = self.select_port(&subset.ports) else { continue };
            let secure = port.name.as_deref() == Some("https");
            for address in &subset.addresses {
                instances.push(
                    ServiceInstance::new(service_name, address.ip.as_str(), port.port)
                        .with_instance_id(format!("{}:{}", address.ip, port.port))
                        .secure(secure),
                );
            }
        }
        instances
    }
}

#[async_trait]
impl ServiceRegistry for KubernetesRegistry {
    async fn register(&self, _instance: &ServiceInstance) -> Result<(), DiscoveryError> {
        Err(Self::read_only("register"))
    }

    async fn deregister(
        &self,
        _service_name: &str,
        _instance_id: &str,
    ) -> Result<(), DiscoveryError> {
        Err(Self::read_only("deregister"))
    }

    async fn heartbeat(
        &self,
        _service_name: &str,
        _instance_id: &str,
    ) -> Result<(), DiscoveryError> {
        Err(Self::read_only("heartbeat"))
    }

    async fn instances(&self, service_name: &str) -> Result<Vec<ServiceInstance>, DiscoveryError> {
        let url = format!(
            "{}/api/v1/namespaces/{}/endpoints/{service_name}",
            self.base_url, self.namespace
        );
        let mut request = self.client.get(url);
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let response = request.send().await.map_err(|e| DiscoveryError::Backend(e.to_string()))?;

        // No Endpoints object ⇒ the service has no ready instances.
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(Vec::new());
        }
        if !response.status().is_success() {
            return Err(DiscoveryError::Backend(format!(
                "kubernetes endpoints lookup returned {}",
                response.status()
            )));
        }

        let endpoints: Endpoints =
            response.json().await.map_err(|e| DiscoveryError::Decode(e.to_string()))?;
        Ok(self.map_endpoints(service_name, &endpoints))
    }
}

// ── Minimal subset of the core/v1 Endpoints resource ──────────────────────────

#[derive(Debug, Default, Deserialize)]
struct Endpoints {
    #[serde(default)]
    subsets: Vec<Subset>,
}

#[derive(Debug, Deserialize)]
struct Subset {
    // Only *ready* addresses appear here; `notReadyAddresses` is intentionally ignored.
    #[serde(default)]
    addresses: Vec<Address>,
    #[serde(default)]
    ports: Vec<EndpointPort>,
}

#[derive(Debug, Deserialize)]
struct Address {
    ip: String,
}

#[derive(Debug, Deserialize)]
struct EndpointPort {
    #[serde(default)]
    name: Option<String>,
    port: u16,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn endpoints_payload() -> serde_json::Value {
        json!({
            "subsets": [{
                "addresses": [{ "ip": "10.1.2.3" }, { "ip": "10.1.2.4" }],
                "ports": [{ "name": "http", "port": 8080 }, { "name": "https", "port": 8443 }]
            }]
        })
    }

    #[tokio::test]
    async fn instances_maps_ready_endpoints() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/namespaces/prod/endpoints/auth"))
            .respond_with(ResponseTemplate::new(200).set_body_json(endpoints_payload()))
            .mount(&server)
            .await;

        let found = KubernetesRegistry::new(server.uri())
            .namespace("prod")
            .instances("auth")
            .await
            .expect("instances");

        // Two ready addresses, first port (http) selected by default.
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].host, "10.1.2.3");
        assert_eq!(found[0].port, 8080);
        assert_eq!(found[0].instance_id, "10.1.2.3:8080");
        assert!(!found[0].secure);
        assert_eq!(found[0].base_url(), "http://10.1.2.3:8080");
    }

    #[tokio::test]
    async fn instances_selects_a_named_port() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/namespaces/default/endpoints/auth"))
            .respond_with(ResponseTemplate::new(200).set_body_json(endpoints_payload()))
            .mount(&server)
            .await;

        let found = KubernetesRegistry::new(server.uri())
            .port_name("https")
            .instances("auth")
            .await
            .expect("instances");

        assert_eq!(found.len(), 2);
        assert_eq!(found[0].port, 8443);
        assert!(found[0].secure, "the https port marks instances secure");
    }

    #[tokio::test]
    async fn unknown_service_404_yields_no_instances() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/namespaces/default/endpoints/missing"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let found =
            KubernetesRegistry::new(server.uri()).instances("missing").await.expect("instances");
        assert!(found.is_empty());
    }

    #[tokio::test]
    async fn mutations_are_read_only_errors() {
        let registry = KubernetesRegistry::new("http://localhost:8001");
        let instance = ServiceInstance::new("auth", "10.0.0.1", 8080);
        assert!(matches!(registry.register(&instance).await, Err(DiscoveryError::Registration(_))));
        assert!(matches!(
            registry.heartbeat("auth", "auth-1").await,
            Err(DiscoveryError::Registration(_))
        ));
    }
}
