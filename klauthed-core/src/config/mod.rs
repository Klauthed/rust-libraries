#![deny(unsafe_code)]

//! Profile-aware application configuration.
//!
//! A service loads configuration once at startup into a resolved [`Config`],
//! then reads typed sections off it synchronously. The source of values is
//! governed by the active [`Profile`]: file/env secrets are permitted in
//! local/dev/test, while staging/prod must source secrets from Vault.
//!
//! ```no_run
//! use klauthed_core::config::Config;
//!
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! // Detect the profile from the environment and load the conventional chain.
//! let config = Config::load().await?;
//!
//! let db = config.database()?;     // typed DatabaseConfig
//! let server = config.server()?;   // typed ServerConfig
//! println!("binding {}", server.bind_address());
//! # let _ = db; Ok(())
//! # }
//! ```

pub mod binding;
pub mod builder;
#[allow(clippy::module_inception)]
pub mod config;
pub mod map;
pub mod profile;
pub mod provider;
#[cfg(feature = "hot-reload")]
pub mod reload;
pub mod schema;

pub use binding::FromConfig;
pub use builder::ConfigBuilder;
pub use config::Config;
pub use map::ConfigMap;
pub use profile::Profile;
pub use provider::{ConfigProvider, ProviderKind};
#[cfg(feature = "hot-reload")]
pub use reload::{RefreshTrigger, ReloadableConfig};
pub use schema::{
    CacheBackend, CacheConfig, DatabaseConfig, DbSystem, KafkaConfig, KafkaSasl, MessagingBackend,
    MessagingConfig, NatsConfig, NatsCredentials, PoolConfig, RabbitMqConfig, ServerConfig,
    StorageConfig,
};

/// Conventional top-level keys for the pre-built typed sections.
pub mod keys {
    /// Key for the [`ServerConfig`](super::ServerConfig) section.
    pub const SERVER: &str = "server";
    /// Key for the [`DatabaseConfig`](super::DatabaseConfig) section.
    pub const DATABASE: &str = "database";
    /// Key for the [`CacheConfig`](super::CacheConfig) section.
    pub const CACHE: &str = "cache";
    /// Key for the [`MessagingConfig`](super::MessagingConfig) section.
    pub const MESSAGING: &str = "messaging";
    /// Key for the [`StorageConfig`](super::StorageConfig) section.
    pub const STORAGE: &str = "storage";
}
