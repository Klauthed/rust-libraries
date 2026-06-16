//! A remote configuration-server **client** [`ConfigProvider`]
//! (`config-server` feature) — fetches a service's config *from* a config server
//! and deep-merges it into the config tree like any other provider.
//!
//! By default it speaks the **klauthed-native** format ([`ConfigServerFormat::Klauthed`]),
//! pairing with our own server,
//! [`klauthed_web::config_server::ConfigServer`](https://docs.rs/klauthed-web) —
//! run a klauthed service as the config server and point this provider at it. It
//! can also speak the [Spring Cloud Config Server] contract
//! ([`SpringCloud`](ConfigServerFormat::SpringCloud)) to consume an existing
//! Spring server, or fetch a plain JSON document
//! ([`RawJson`](ConfigServerFormat::RawJson)).
//!
//! It is a **non-secret** source ([`ProviderKind::ConfigServer`]): use it for
//! configuration, and keep secrets in Vault.
//!
//! [Spring Cloud Config Server]: https://docs.spring.io/spring-cloud-config/reference/server.html

use std::collections::BTreeMap;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;

use super::{ConfigProvider, ProviderKind};
use crate::config::map::ConfigMap;
use crate::error::ConfigError;

/// The wire format a configuration server speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConfigServerFormat {
    /// The **klauthed-native** config server
    /// (`klauthed_web::config_server::ConfigServer`):
    /// `GET /{application}/{profile}[/{label}]` returns a `ConfigDocument` whose
    /// `config` field is the (already nested) configuration tree. The default —
    /// this is what our own server speaks.
    #[default]
    Klauthed,
    /// Spring Cloud Config Server: `GET /{application}/{profile}[/{label}]`
    /// returns ordered `propertySources` whose `source` maps hold flat, dotted
    /// keys (e.g. `database.host`). For talking to an existing Spring server.
    SpringCloud,
    /// A plain JSON object fetched verbatim from the base URL — for config
    /// stored as a single document by some other tool.
    RawJson,
}

/// How the provider authenticates to the config server.
#[derive(Clone)]
enum Auth {
    Basic { username: String, password: String },
    Bearer(String),
}

/// A [`ConfigProvider`] that loads configuration from a remote HTTP config
/// server.
///
/// ```no_run
/// use klauthed_core::config::provider::ConfigServerProvider;
///
/// // Spring Cloud Config Server at config.internal, app "auth-api", profile "prod".
/// let provider = ConfigServerProvider::new("https://config.internal", "auth-api")
///     .profile("prod")
///     .bearer_auth("…token…")
///     .optional(false);
/// # let _ = provider;
/// ```
pub struct ConfigServerProvider {
    base_url: String,
    application: String,
    profile: String,
    label: Option<String>,
    format: ConfigServerFormat,
    auth: Option<Auth>,
    optional: bool,
    client: reqwest::Client,
}

impl ConfigServerProvider {
    /// A provider for `application` against the server at `base_url`, profile
    /// `default`, klauthed-native format, required (not optional).
    #[must_use]
    pub fn new(base_url: impl Into<String>, application: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_owned(),
            application: application.into(),
            profile: "default".to_owned(),
            label: None,
            format: ConfigServerFormat::default(),
            auth: None,
            optional: false,
            client: reqwest::Client::new(),
        }
    }

    /// Set the profile (e.g. `"prod"`) requested from the server.
    #[must_use]
    pub fn profile(mut self, profile: impl Into<String>) -> Self {
        self.profile = profile.into();
        self
    }

    /// Set the Spring Cloud label (git branch/tag); omitted by default.
    #[must_use]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Choose the wire [`ConfigServerFormat`].
    #[must_use]
    pub fn format(mut self, format: ConfigServerFormat) -> Self {
        self.format = format;
        self
    }

    /// Use the klauthed-native config-server format (shorthand for
    /// [`format`](Self::format) with [`ConfigServerFormat::Klauthed`]). This is
    /// already the default; call it to be explicit.
    #[must_use]
    pub fn klauthed(mut self) -> Self {
        self.format = ConfigServerFormat::Klauthed;
        self
    }

    /// Speak the Spring Cloud Config Server contract (shorthand for
    /// [`format`](Self::format) with [`ConfigServerFormat::SpringCloud`]).
    #[must_use]
    pub fn spring_cloud(mut self) -> Self {
        self.format = ConfigServerFormat::SpringCloud;
        self
    }

    /// Fetch a plain JSON document from the base URL instead of the Spring Cloud
    /// contract (shorthand for [`format`](Self::format) with
    /// [`ConfigServerFormat::RawJson`]).
    #[must_use]
    pub fn raw_json(mut self) -> Self {
        self.format = ConfigServerFormat::RawJson;
        self
    }

    /// Authenticate with HTTP Basic credentials.
    #[must_use]
    pub fn basic_auth(mut self, username: impl Into<String>, password: impl Into<String>) -> Self {
        self.auth = Some(Auth::Basic { username: username.into(), password: password.into() });
        self
    }

    /// Authenticate with a bearer token.
    #[must_use]
    pub fn bearer_auth(mut self, token: impl Into<String>) -> Self {
        self.auth = Some(Auth::Bearer(token.into()));
        self
    }

    /// When `true`, an unreachable or erroring server yields an empty
    /// contribution (logged) instead of failing the load. Defaults to `false`
    /// (fail-fast at startup).
    #[must_use]
    pub fn optional(mut self, optional: bool) -> Self {
        self.optional = optional;
        self
    }

    /// Use a pre-built [`reqwest::Client`] (custom timeouts, TLS, …).
    #[must_use]
    pub fn with_client(mut self, client: reqwest::Client) -> Self {
        self.client = client;
        self
    }

    /// The URL this provider fetches.
    fn url(&self) -> String {
        match self.format {
            ConfigServerFormat::SpringCloud | ConfigServerFormat::Klauthed => match &self.label {
                Some(label) => {
                    format!("{}/{}/{}/{label}", self.base_url, self.application, self.profile)
                }
                None => format!("{}/{}/{}", self.base_url, self.application, self.profile),
            },
            ConfigServerFormat::RawJson => self.base_url.clone(),
        }
    }

    /// Handle a transport/HTTP failure per the `optional` policy.
    fn on_failure(&self, url: &str, message: String) -> Result<ConfigMap, ConfigError> {
        if self.optional {
            tracing::warn!(%url, %message, "optional config server unavailable; continuing without it");
            Ok(ConfigMap::new())
        } else {
            Err(ConfigError::ConfigServer { url: url.to_owned(), message })
        }
    }
}

#[async_trait]
impl ConfigProvider for ConfigServerProvider {
    fn name(&self) -> String {
        format!("config-server:{}", self.url())
    }

    fn kind(&self) -> ProviderKind {
        ProviderKind::ConfigServer
    }

    async fn load(&self) -> Result<ConfigMap, ConfigError> {
        let url = self.url();

        let mut request = self.client.get(&url);
        request = match &self.auth {
            Some(Auth::Basic { username, password }) => {
                request.basic_auth(username, Some(password))
            }
            Some(Auth::Bearer(token)) => request.bearer_auth(token),
            None => request,
        };

        let response = match request.send().await {
            Ok(response) => response,
            Err(error) => return self.on_failure(&url, error.to_string()),
        };

        if !response.status().is_success() {
            return self.on_failure(&url, format!("HTTP {}", response.status()));
        }

        match self.format {
            ConfigServerFormat::Klauthed => {
                let document: KlauthedDocument = response.json().await.map_err(|e| {
                    ConfigError::ConfigServer { url: url.clone(), message: e.to_string() }
                })?;
                match document.config {
                    Value::Object(map) => {
                        Ok(ConfigMap::from(map.into_iter().collect::<BTreeMap<_, _>>()))
                    }
                    Value::Null => Ok(ConfigMap::new()),
                    other => Err(ConfigError::ConfigServer {
                        url,
                        message: format!("expected `config` to be a JSON object, got {other}"),
                    }),
                }
            }
            ConfigServerFormat::SpringCloud => {
                let parsed: SpringCloudResponse = response
                    .json()
                    .await
                    .map_err(|e| ConfigError::ConfigServer { url, message: e.to_string() })?;
                Ok(parsed.into_config_map())
            }
            ConfigServerFormat::RawJson => {
                let value: Value = response.json().await.map_err(|e| {
                    ConfigError::ConfigServer { url: url.clone(), message: e.to_string() }
                })?;
                match value {
                    Value::Object(map) => {
                        Ok(ConfigMap::from(map.into_iter().collect::<BTreeMap<_, _>>()))
                    }
                    _ => Err(ConfigError::ConfigServer {
                        url,
                        message: "expected a top-level JSON object".to_owned(),
                    }),
                }
            }
        }
    }
}

/// The klauthed-native `ConfigDocument`; only the `config` tree is needed here.
#[derive(Deserialize)]
struct KlauthedDocument {
    #[serde(default)]
    config: Value,
}

#[derive(Deserialize)]
struct SpringCloudResponse {
    #[serde(rename = "propertySources", default)]
    property_sources: Vec<PropertySource>,
}

#[derive(Deserialize)]
struct PropertySource {
    #[serde(default)]
    source: BTreeMap<String, Value>,
}

impl SpringCloudResponse {
    fn into_config_map(self) -> ConfigMap {
        // `propertySources` are ordered highest-precedence first. Insert the
        // lowest precedence first so a higher-precedence source overrides it at
        // the leaf-key level, then expand the flat dotted keys into a tree.
        let mut flat: BTreeMap<String, Value> = BTreeMap::new();
        for source in self.property_sources.into_iter().rev() {
            flat.extend(source.source);
        }
        ConfigMap::from(flat).expand_dotted()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn spring_cloud_merges_and_nests_property_sources() {
        let server = MockServer::start().await;
        // Two sources: the first (highest precedence) overrides `database.port`.
        let body = json!({
            "name": "auth-api",
            "profiles": ["prod"],
            "propertySources": [
                { "name": "overrides", "source": { "database.port": 6543 } },
                {
                    "name": "base",
                    "source": {
                        "database.host": "db.internal",
                        "database.port": 5432,
                        "app_name": "auth"
                    }
                }
            ]
        });
        Mock::given(method("GET"))
            .and(path("/auth-api/prod"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;

        let map = ConfigServerProvider::new(server.uri(), "auth-api")
            .profile("prod")
            .spring_cloud()
            .load()
            .await
            .expect("load");

        assert_eq!(map.get("app_name"), Some(&json!("auth")));
        assert_eq!(map.get("database"), Some(&json!({ "host": "db.internal", "port": 6543 })));
    }

    #[tokio::test]
    async fn klauthed_format_extracts_the_config_tree() {
        let server = MockServer::start().await;
        // Native ConfigDocument: `config` is the already-nested tree.
        let body = json!({
            "application": "auth-api",
            "profile": "prod",
            "config": { "database": { "host": "db.internal", "port": 6543 }, "app_name": "auth" }
        });
        Mock::given(method("GET"))
            .and(path("/auth-api/prod"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;

        // Klauthed is the default format — no `.klauthed()` needed.
        let map = ConfigServerProvider::new(server.uri(), "auth-api")
            .profile("prod")
            .load()
            .await
            .unwrap();

        assert_eq!(map.get("app_name"), Some(&json!("auth")));
        assert_eq!(map.get("database"), Some(&json!({ "host": "db.internal", "port": 6543 })));
    }

    #[tokio::test]
    async fn label_is_appended_to_the_path() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/auth-api/prod/v2"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({ "propertySources": [] })),
            )
            .mount(&server)
            .await;

        ConfigServerProvider::new(server.uri(), "auth-api")
            .profile("prod")
            .label("v2")
            .load()
            .await
            .expect("load");
    }

    #[tokio::test]
    async fn raw_json_loads_the_document_verbatim() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "database": { "host": "db" }, "debug": true })),
            )
            .mount(&server)
            .await;

        let map = ConfigServerProvider::new(server.uri(), "ignored")
            .raw_json()
            .load()
            .await
            .expect("load");
        assert_eq!(map.get("database"), Some(&json!({ "host": "db" })));
        assert_eq!(map.get("debug"), Some(&json!(true)));
    }

    #[tokio::test]
    async fn bearer_auth_is_sent() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(header("authorization", "Bearer s3cret"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({ "propertySources": [] })),
            )
            .mount(&server)
            .await;

        ConfigServerProvider::new(server.uri(), "auth-api")
            .bearer_auth("s3cret")
            .load()
            .await
            .expect("authenticated load");
    }

    #[tokio::test]
    async fn optional_swallows_a_server_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(500)).mount(&server).await;

        // Required → error.
        let required = ConfigServerProvider::new(server.uri(), "auth-api").load().await;
        assert!(matches!(required, Err(ConfigError::ConfigServer { .. })));

        // Optional → empty contribution.
        let optional = ConfigServerProvider::new(server.uri(), "auth-api")
            .optional(true)
            .load()
            .await
            .expect("optional load");
        assert!(optional.is_empty());
    }
}
