//! Broker-agnostic messaging / event-bus configuration.
//!
//! [`MessagingConfig`] is tagged on `backend`, so a service can switch brokers
//! by changing config alone:
//!
//! ```toml
//! [messaging]
//! backend = "nats"          # or "rabbitmq", "kafka"
//! urls    = ["nats://localhost:4222"]
//! jetstream = true
//! ```
//!
//! Each variant carries its own broker-specific connection struct, so today's
//! NATS deployment can become RabbitMQ or Kafka later without touching the
//! call sites that read `config.messaging()`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The configured messaging backend and its connection details.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "backend", rename_all = "snake_case")]
pub enum MessagingConfig {
    /// NATS (optionally JetStream).
    Nats(NatsConfig),
    /// RabbitMQ (AMQP 0-9-1).
    #[serde(rename = "rabbitmq")]
    RabbitMq(RabbitMqConfig),
    /// Apache Kafka.
    Kafka(KafkaConfig),
}

impl Default for MessagingConfig {
    fn default() -> Self {
        MessagingConfig::Nats(NatsConfig::default())
    }
}

impl MessagingConfig {
    /// Which backend this config selects, without its payload.
    pub fn backend(&self) -> MessagingBackend {
        match self {
            MessagingConfig::Nats(_) => MessagingBackend::Nats,
            MessagingConfig::RabbitMq(_) => MessagingBackend::RabbitMq,
            MessagingConfig::Kafka(_) => MessagingBackend::Kafka,
        }
    }
}

/// The set of supported messaging backends, as a flat tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MessagingBackend {
    #[default]
    Nats,
    #[serde(rename = "rabbitmq")]
    RabbitMq,
    Kafka,
}

// ── NATS ──────────────────────────────────────────────────────────────────────

/// How a service authenticates to NATS.
///
/// Tagged on `type`, e.g.
/// `{ "type": "user_password", "username": "svc", "password": "..." }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NatsCredentials {
    /// No authentication.
    #[default]
    None,
    /// Token auth.
    Token { token: String },
    /// User/password auth.
    UserPassword { username: String, password: String },
    /// Path to a NATS `.creds` file (JWT + nkey seed).
    CredsFile { path: PathBuf },
    /// Raw nkey seed.
    NKey { seed: String },
}

/// NATS connection settings (core NATS and, optionally, JetStream).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NatsConfig {
    /// One or more server URLs. Multiple entries form a cluster seed list.
    #[serde(default = "default_nats_urls")]
    pub urls: Vec<String>,
    /// Optional connection name (shown in NATS monitoring).
    #[serde(default)]
    pub name: Option<String>,
    /// Authentication. Prefer sourcing secrets from Vault in staging/prod.
    #[serde(default)]
    pub credentials: NatsCredentials,
    /// Use TLS for the connection.
    #[serde(default)]
    pub tls: bool,
    /// Connection timeout in seconds.
    #[serde(default = "default_connect_timeout_secs")]
    pub connect_timeout_secs: u64,
    /// Maximum reconnection attempts (`0` = unlimited).
    #[serde(default = "default_max_reconnects")]
    pub max_reconnects: u32,
    /// Enable JetStream features.
    #[serde(default = "default_jetstream")]
    pub jetstream: bool,
}

fn default_nats_urls() -> Vec<String> {
    vec!["nats://localhost:4222".to_owned()]
}
fn default_connect_timeout_secs() -> u64 {
    10
}
fn default_max_reconnects() -> u32 {
    60
}
fn default_jetstream() -> bool {
    true
}

impl Default for NatsConfig {
    fn default() -> Self {
        Self {
            urls: default_nats_urls(),
            name: None,
            credentials: NatsCredentials::default(),
            tls: false,
            connect_timeout_secs: default_connect_timeout_secs(),
            max_reconnects: default_max_reconnects(),
            jetstream: default_jetstream(),
        }
    }
}

// ── RabbitMQ ──────────────────────────────────────────────────────────────────

/// RabbitMQ (AMQP) connection settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RabbitMqConfig {
    /// Full AMQP URI; when set it overrides the component fields.
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default = "default_rabbit_host")]
    pub host: String,
    #[serde(default = "default_rabbit_port")]
    pub port: u16,
    /// Virtual host (default `/`).
    #[serde(default = "default_vhost")]
    pub vhost: String,
    #[serde(default)]
    pub username: Option<String>,
    /// Password. Prefer sourcing this from Vault in staging/prod.
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub tls: bool,
    #[serde(default = "default_connect_timeout_secs")]
    pub connect_timeout_secs: u64,
}

fn default_rabbit_host() -> String {
    "localhost".to_owned()
}
fn default_rabbit_port() -> u16 {
    5672
}
fn default_vhost() -> String {
    "/".to_owned()
}

impl Default for RabbitMqConfig {
    fn default() -> Self {
        Self {
            url: None,
            host: default_rabbit_host(),
            port: default_rabbit_port(),
            vhost: default_vhost(),
            username: None,
            password: None,
            tls: false,
            connect_timeout_secs: default_connect_timeout_secs(),
        }
    }
}

impl RabbitMqConfig {
    /// Build an AMQP URI from the components, or return `url` verbatim if set.
    pub fn connection_url(&self) -> String {
        if let Some(url) = &self.url {
            return url.clone();
        }
        let scheme = if self.tls { "amqps" } else { "amqp" };
        let mut url = format!("{scheme}://");
        if let Some(user) = &self.username {
            url.push_str(user);
            if let Some(password) = &self.password {
                url.push(':');
                url.push_str(password);
            }
            url.push('@');
        }
        url.push_str(&self.host);
        url.push(':');
        url.push_str(&self.port.to_string());
        // vhost "/" is encoded as an empty path segment; a named vhost follows the slash.
        url.push('/');
        if self.vhost != "/" {
            url.push_str(self.vhost.trim_start_matches('/'));
        }
        url
    }
}

// ── Kafka ─────────────────────────────────────────────────────────────────────

/// SASL credentials for Kafka.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KafkaSasl {
    /// SASL mechanism, e.g. `PLAIN`, `SCRAM-SHA-256`, `SCRAM-SHA-512`.
    pub mechanism: String,
    pub username: String,
    /// Password. Prefer sourcing this from Vault in staging/prod.
    pub password: String,
}

/// Apache Kafka connection settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KafkaConfig {
    /// Bootstrap broker list, e.g. `["broker-1:9092", "broker-2:9092"]`.
    #[serde(default = "default_kafka_brokers")]
    pub brokers: Vec<String>,
    /// Client identifier reported to the cluster.
    #[serde(default)]
    pub client_id: Option<String>,
    /// Consumer group id (for consumers).
    #[serde(default)]
    pub group_id: Option<String>,
    #[serde(default)]
    pub tls: bool,
    /// SASL credentials, if the cluster requires them.
    #[serde(default)]
    pub sasl: Option<KafkaSasl>,
}

fn default_kafka_brokers() -> Vec<String> {
    vec!["localhost:9092".to_owned()]
}

impl Default for KafkaConfig {
    fn default() -> Self {
        Self {
            brokers: default_kafka_brokers(),
            client_id: None,
            group_id: None,
            tls: false,
            sasl: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults_to_nats() {
        let cfg = MessagingConfig::default();
        assert_eq!(cfg.backend(), MessagingBackend::Nats);
    }

    #[test]
    fn parses_nats_backend_with_credentials() {
        let cfg: MessagingConfig = serde_json::from_value(json!({
            "backend": "nats",
            "urls": ["nats://a:4222", "nats://b:4222"],
            "jetstream": true,
            "credentials": { "type": "user_password", "username": "svc", "password": "pw" }
        }))
        .unwrap();

        match cfg {
            MessagingConfig::Nats(nats) => {
                assert_eq!(nats.urls.len(), 2);
                assert!(nats.jetstream);
                assert_eq!(
                    nats.credentials,
                    NatsCredentials::UserPassword { username: "svc".into(), password: "pw".into() }
                );
            }
            other => panic!("expected NATS, got {other:?}"),
        }
    }

    #[test]
    fn parses_rabbitmq_backend_and_builds_uri() {
        let cfg: MessagingConfig = serde_json::from_value(json!({
            "backend": "rabbitmq",
            "host": "rabbit",
            "username": "svc",
            "password": "pw",
            "vhost": "app"
        }))
        .unwrap();

        let MessagingConfig::RabbitMq(rabbit) = cfg else {
            panic!("expected RabbitMQ");
        };
        assert_eq!(rabbit.connection_url(), "amqp://svc:pw@rabbit:5672/app");
    }

    #[test]
    fn parses_kafka_backend() {
        let cfg: MessagingConfig = serde_json::from_value(json!({
            "backend": "kafka",
            "brokers": ["k1:9092", "k2:9092"],
            "group_id": "svc"
        }))
        .unwrap();

        let MessagingConfig::Kafka(kafka) = cfg else {
            panic!("expected Kafka");
        };
        assert_eq!(kafka.brokers, vec!["k1:9092", "k2:9092"]);
        assert_eq!(kafka.group_id.as_deref(), Some("svc"));
    }
}
