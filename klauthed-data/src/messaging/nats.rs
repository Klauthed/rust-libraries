//! NATS connection from a [`MessagingConfig`].

use std::time::Duration;

use async_nats::{Client, ConnectOptions};
use klauthed_core::config::{MessagingConfig, NatsConfig, NatsCredentials};

use crate::error::DataError;

/// Connect to NATS using the `nats` variant of `config`.
///
/// Returns [`DataError::UnsupportedMessagingBackend`] if the config selects a
/// different broker. Credentials, TLS, the connection timeout and the reconnect
/// budget are all taken from the typed config.
pub async fn connect(config: &MessagingConfig) -> Result<Client, DataError> {
    let MessagingConfig::Nats(nats) = config else {
        return Err(DataError::UnsupportedMessagingBackend(config.backend()));
    };

    let options = build_options(nats).await?;
    tracing::debug!(servers = nats.urls.len(), "connecting to NATS");
    let client = options.connect(nats.urls.clone()).await?;
    Ok(client)
}

/// Assemble [`ConnectOptions`] from a [`NatsConfig`].
async fn build_options(nats: &NatsConfig) -> Result<ConnectOptions, DataError> {
    let mut options = match &nats.credentials {
        NatsCredentials::None => ConnectOptions::new(),
        NatsCredentials::Token { token } => ConnectOptions::new().token(token.clone()),
        NatsCredentials::UserPassword { username, password } => {
            ConnectOptions::new().user_and_password(username.clone(), password.clone())
        }
        NatsCredentials::NKey { seed } => ConnectOptions::new().nkey(seed.clone()),
        NatsCredentials::CredsFile { path } => {
            ConnectOptions::with_credentials_file(path).await.map_err(|e| {
                DataError::Messaging(format!(
                    "reading NATS credentials file '{}': {e}",
                    path.display()
                ))
            })?
        }
    };

    if let Some(name) = &nats.name {
        options = options.name(name);
    }
    if nats.tls {
        options = options.require_tls(true);
    }
    options = options.connection_timeout(Duration::from_secs(nats.connect_timeout_secs));
    // 0 in our config means "unlimited"; async-nats models that as `None`.
    let budget = (nats.max_reconnects > 0).then_some(nats.max_reconnects as usize);
    options = options.max_reconnects(budget);

    Ok(options)
}

#[cfg(test)]
mod tests {
    use super::*;
    use klauthed_core::config::{KafkaConfig, MessagingConfig};

    #[tokio::test]
    async fn rejects_non_nats_backend() {
        let config = MessagingConfig::Kafka(KafkaConfig::default());
        let err = connect(&config).await.unwrap_err();
        assert!(matches!(err, DataError::UnsupportedMessagingBackend(_)));
    }

    #[tokio::test]
    async fn builds_options_for_token_credentials() {
        // Exercises the option-building path without opening a socket.
        let nats = NatsConfig {
            name: Some("svc".into()),
            credentials: NatsCredentials::Token { token: "t".into() },
            ..Default::default()
        };
        build_options(&nats).await.expect("options build");
    }
}
