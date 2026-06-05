//! Pre-built, strongly-typed configuration sections.
//!
//! These are plain serde data structures describing the things almost every
//! backend service needs to configure — database, cache, messaging, object
//! storage, HTTP server. They carry sensible defaults so a service only has to
//! specify what differs from the norm, and they are deliberately free of any
//! driver/client dependency: actually opening pools and connections is the job
//! of the higher-level crates (e.g. `klauthed-data`), not the config layer.
//!
//! Read them off a [`Config`](crate::config::Config) with the convenience
//! accessors (`config.database()?`) or generically (`config.get("database")?`).

pub mod cache;
pub mod database;
pub mod messaging;
pub mod server;
pub mod storage;

pub use cache::{CacheBackend, CacheConfig};
pub use database::{DatabaseConfig, DbSystem, PoolConfig};
pub use messaging::{
    KafkaConfig, KafkaSasl, MessagingBackend, MessagingConfig, NatsConfig, NatsCredentials,
    RabbitMqConfig,
};
pub use server::ServerConfig;
pub use storage::StorageConfig;
