//! MongoDB client connector.
//!
//! Provides [`connect`] and [`connect_verified`] for building a
//! [`mongodb::Client`] from a [`DatabaseConfig`], and a [`ping`] helper for
//! health-check use.
//!
//! The expected connection URI is any URI accepted by the official MongoDB Rust
//! driver (e.g. `mongodb://user:pass@host:27017/?authSource=admin` or a
//! `mongodb+srv://…` DNS seed-list string).  If [`DatabaseConfig::url`] is
//! `None`, the driver default `mongodb://127.0.0.1:27017` is used.
//!
//! Live tests are marked `#[ignore]`; run them with a MongoDB instance
//! available at `MONGODB_URL` via:
//! ```text
//! cargo test -p klauthed-data --features mongodb -- --ignored
//! ```

use std::time::Duration;

use klauthed_core::config::DatabaseConfig;
use mongodb::Client;
use mongodb::bson::doc;
use mongodb::options::ClientOptions;

use crate::error::DataError;

/// Connect to MongoDB using the URI in `config.url`.
///
/// Sets `server_selection_timeout` from `config.pool.acquire_timeout_secs`.
/// Returns a connected (but not verified) [`Client`].
pub async fn connect(config: &DatabaseConfig) -> Result<Client, DataError> {
    let uri = config
        .url
        .as_deref()
        .unwrap_or("mongodb://127.0.0.1:27017");

    let mut opts = ClientOptions::parse(uri)
        .await
        .map_err(|e| DataError::Outbox(format!("mongodb URI parse error: {e}")))?;

    opts.server_selection_timeout =
        Some(Duration::from_secs(config.pool.acquire_timeout_secs));

    Client::with_options(opts)
        .map_err(|e| DataError::Outbox(format!("mongodb client error: {e}")))
}

/// Connect to MongoDB and verify the server responds to a `ping` command.
///
/// Fails fast at startup on misconfiguration.
pub async fn connect_verified(config: &DatabaseConfig) -> Result<Client, DataError> {
    let client = connect(config).await?;
    ping(&client).await?;
    Ok(client)
}

/// Ping the MongoDB server by running `{ping: 1}` against the `admin` database.
pub async fn ping(client: &Client) -> Result<(), DataError> {
    client
        .database("admin")
        .run_command(doc! { "ping": 1 })
        .await
        .map_err(|e| DataError::Outbox(format!("mongodb ping failed: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use klauthed_core::config::DatabaseConfig;

    #[tokio::test]
    #[ignore = "requires a live MongoDB at MONGODB_URL"]
    async fn connect_verified_and_ping() {
        let url = std::env::var("MONGODB_URL")
            .unwrap_or_else(|_| "mongodb://127.0.0.1:27017".to_owned());
        let config = DatabaseConfig {
            url: Some(url),
            ..Default::default()
        };
        let client = connect_verified(&config).await.expect("connect+ping");
        ping(&client).await.expect("second ping");
    }
}
