#![deny(unsafe_code)]
#![deny(missing_docs)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)
)]

//! Data-layer connectors for klauthed services.
//!
//! This crate turns the typed configuration sections from
//! [`klauthed_core::config`] into **real, connected resources** — database
//! pools, cache clients — so a service never hand-rolls connection wiring.
//!
//! Every backend lives behind a Cargo feature, so a service compiles only the
//! drivers it actually uses:
//!
//! (Plain code spans below rather than intra-doc links, since these items are
//! feature-gated and absent from a default-feature doc build.)
//!
//! | Feature        | Provides                                              |
//! |----------------|-------------------------------------------------------|
//! | `postgres`     | `db::connect` for PostgreSQL (implies `sql`)         |
//! | `mysql`        | `db::connect` for MySQL/MariaDB (implies `sql`)      |
//! | `sqlite`       | `db::connect` for SQLite (implies `sql`)             |
//! | `redis`        | `cache::connect_redis`                               |
//! | `cache-memory` | `cache::build_memory_cache` (moka, in-process)      |
//! | `nats`         | `messaging::connect_nats` (async-nats)              |
//! | `rabbitmq`     | `messaging::connect_rabbitmq` (lapin / AMQP)        |
//! | `kafka`        | `messaging::connect_kafka` (rskafka, pure Rust)     |
//! | `storage`      | `storage::connect` for local filesystem             |
//! | `storage-s3`   | `storage::connect` for S3 / S3-compatible           |
//! | `storage-gcs`  | `storage::connect` for Google Cloud Storage         |
//! | `storage-azure`| `storage::connect` for Azure Blob Storage           |
//! | `mongodb`      | `db::mongo::connect` for MongoDB                    |
//!
//! ```no_run
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! use klauthed_core::config::Config;
//!
//! let config = Config::load().await?;
//!
//! # #[cfg(feature = "sql")]
//! let pool = klauthed_data::db::connect(&config.database()?).await?;
//! # Ok(())
//! # }
//! ```

pub mod error;

// ── Reliability patterns ──────────────────────────────────────────────────────
//
// Backend-agnostic traits with in-memory implementations. These are always
// compiled (no feature gate) since they carry no driver dependencies; real
// Postgres/Redis backends are in sub-modules gated by their own features.

pub mod idempotency;
pub mod locks;
pub mod outbox;
pub mod rate_limit;

// The `db` module houses the relational connector (sql feature) and the
// MongoDB sub-module (mongodb feature). It is compiled whenever any of those
// features is active.
#[cfg(any(feature = "sql", feature = "mongodb"))]
pub mod db;

#[cfg(any(feature = "redis", feature = "cache-memory"))]
pub mod cache;

#[cfg(any(feature = "nats", feature = "rabbitmq", feature = "kafka"))]
pub mod messaging;

#[cfg(feature = "storage")]
pub mod storage;

// Embedded, versioned schema migrations over a relational pool.
#[cfg(feature = "sql")]
pub mod migrate;

// Auto-configuration starter: build the SQL pool from config into an AppContext.
#[cfg(feature = "sql")]
pub mod starter;

pub mod pagination;
pub mod saga;

// ── Stub modules reserved for future implementation ───────────────────────────
pub mod eventbus;
pub mod transaction;

pub use error::DataError;

#[cfg(feature = "sql")]
pub use migrate::{Migration, Migrator};
#[cfg(feature = "sql")]
pub use sqlx::AnyPool;
#[cfg(feature = "sql")]
pub use starter::DataStarter;

pub use idempotency::{
    IdempotencyRecord, IdempotencyStatus, IdempotencyStore, InMemoryIdempotencyStore, Outcome,
};
pub use locks::{InMemoryLockManager, LockGuard, LockManager, LockToken};
pub use outbox::{InMemoryOutbox, Outbox, OutboxEntry, OutboxId, OutboxPublisher, OutboxRelay};
pub use saga::{Saga, SagaError};

/// Common imports for the data layer: `use klauthed_data::prelude::*;`.
pub mod prelude {
    #[cfg(feature = "sql")]
    pub use crate::{AnyPool, DataStarter, Migration, Migrator};
    pub use crate::{
        DataError, IdempotencyStore, InMemoryIdempotencyStore, InMemoryLockManager, InMemoryOutbox,
        LockGuard, LockManager, LockToken, Outbox, OutboxEntry, OutboxId, OutboxPublisher,
        OutboxRelay, Saga, SagaError,
    };
}

#[cfg(feature = "sql")]
pub use outbox::SqlOutbox;

#[cfg(feature = "redis")]
pub use locks::redis::RedisLockManager;

#[cfg(feature = "redis")]
pub use idempotency::redis::RedisIdempotencyStore;

#[cfg(feature = "mongodb")]
pub use outbox::MongoOutbox;

#[cfg(feature = "mongodb")]
pub use locks::mongo::MongoLockManager;

#[cfg(feature = "mongodb")]
pub use idempotency::mongo::MongoIdempotencyStore;
