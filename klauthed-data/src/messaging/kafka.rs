//! Kafka connection from a [`MessagingConfig`].
//!
//! Uses the pure-Rust `rskafka` client, so no `librdkafka` / C toolchain is
//! required. Returns a cluster-wide [`Client`] from which partition/controller
//! sub-clients are derived.

use klauthed_core::config::{KafkaSasl, MessagingConfig};
use rskafka::client::{Client, ClientBuilder, Credentials, SaslConfig};

use crate::error::DataError;

/// Connect to Kafka using the `kafka` variant of `config`.
///
/// Returns [`DataError::UnsupportedMessagingBackend`] for a different broker.
/// TLS is reported as unsupported rather than silently downgraded — wiring it
/// needs a `rustls::ClientConfig`, which this connector does not yet build.
pub async fn connect(config: &MessagingConfig) -> Result<Client, DataError> {
    let MessagingConfig::Kafka(kafka) = config else {
        return Err(DataError::UnsupportedMessagingBackend(config.backend()));
    };

    if kafka.tls {
        return Err(DataError::Messaging(
            "Kafka TLS requires a rustls ClientConfig, which this connector does not yet build; \
             use a plaintext listener or extend the connector"
                .to_owned(),
        ));
    }

    let mut builder = ClientBuilder::new(kafka.brokers.clone());
    if let Some(client_id) = &kafka.client_id {
        builder = builder.client_id(client_id.clone());
    }
    if let Some(sasl) = &kafka.sasl {
        builder = builder.sasl_config(sasl_config(sasl)?);
    }

    tracing::debug!(brokers = kafka.brokers.len(), "connecting to Kafka");
    let client = builder.build().await?;
    Ok(client)
}

/// Map our [`KafkaSasl`] onto rskafka's [`SaslConfig`].
fn sasl_config(sasl: &KafkaSasl) -> Result<SaslConfig, DataError> {
    let credentials = Credentials::new(sasl.username.clone(), sasl.password.clone());
    match sasl.mechanism.to_ascii_uppercase().as_str() {
        "PLAIN" => Ok(SaslConfig::Plain(credentials)),
        "SCRAM-SHA-256" => Ok(SaslConfig::ScramSha256(credentials)),
        "SCRAM-SHA-512" => Ok(SaslConfig::ScramSha512(credentials)),
        other => Err(DataError::Messaging(format!("unsupported Kafka SASL mechanism: {other}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use klauthed_core::config::{KafkaConfig, NatsConfig};

    #[tokio::test]
    async fn rejects_non_kafka_backend() {
        let config = MessagingConfig::Nats(NatsConfig::default());
        let err = connect(&config).await.unwrap_err();
        assert!(matches!(err, DataError::UnsupportedMessagingBackend(_)));
    }

    #[tokio::test]
    async fn rejects_tls_until_supported() {
        let config = MessagingConfig::Kafka(KafkaConfig { tls: true, ..Default::default() });
        let err = connect(&config).await.unwrap_err();
        assert!(matches!(err, DataError::Messaging(_)));
    }

    #[test]
    fn maps_known_sasl_mechanisms() {
        let sasl = KafkaSasl {
            mechanism: "scram-sha-256".into(),
            username: "u".into(),
            password: "p".into(),
        };
        assert!(matches!(sasl_config(&sasl), Ok(SaslConfig::ScramSha256(_))));

        let bad =
            KafkaSasl { mechanism: "kerberos".into(), username: "u".into(), password: "p".into() };
        assert!(matches!(sasl_config(&bad), Err(DataError::Messaging(_))));
    }
}
