//! RabbitMQ (AMQP) connection from a [`MessagingConfig`].

use klauthed_core::config::MessagingConfig;
use lapin::{Connection, ConnectionProperties};

use crate::error::DataError;

/// Open an AMQP connection using the `rabbitmq` variant of `config`.
///
/// The AMQP URI comes from
/// [`RabbitMqConfig::connection_url`](klauthed_core::config::RabbitMqConfig::connection_url),
/// so either the explicit `url` or the host/port/vhost/credentials components
/// are honored.
/// lapin 4.x integrates with the ambient tokio runtime, so no executor setup is
/// required.
pub async fn connect_rabbitmq(config: &MessagingConfig) -> Result<Connection, DataError> {
    let MessagingConfig::RabbitMq(rabbit) = config else {
        return Err(DataError::UnsupportedMessagingBackend(config.backend()));
    };

    let uri = rabbit.connection_url();
    tracing::debug!("connecting to RabbitMQ");
    let connection = Connection::connect(&uri, ConnectionProperties::default()).await?;
    Ok(connection)
}

#[cfg(test)]
mod tests {
    use super::*;
    use klauthed_core::config::NatsConfig;

    #[tokio::test]
    async fn rejects_non_rabbitmq_backend() {
        let config = MessagingConfig::Nats(NatsConfig::default());
        let err = connect_rabbitmq(&config).await.unwrap_err();
        assert!(matches!(err, DataError::UnsupportedMessagingBackend(_)));
    }
}
