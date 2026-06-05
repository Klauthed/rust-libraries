#![deny(unsafe_code)]

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
// Postgres/Redis backends are a future pass.

pub mod idempotency;
pub mod locks;
pub mod outbox;

#[cfg(feature = "sql")]
pub mod db;

/// SQL-backed [`Outbox`] over a driver-agnostic `sqlx::AnyPool`.
#[cfg(feature = "sql")]
pub mod outbox_sql;

#[cfg(any(feature = "redis", feature = "cache-memory"))]
pub mod cache;

/// Redis-backed [`LockManager`] (`SET … NX PX` + compare-and-delete Lua).
#[cfg(feature = "redis")]
pub mod locks_redis;

/// Redis-backed [`IdempotencyStore`] (`SET … NX PX` for atomic claim).
#[cfg(feature = "redis")]
pub mod idempotency_redis;

#[cfg(any(feature = "nats", feature = "rabbitmq", feature = "kafka"))]
pub mod messaging;

#[cfg(feature = "storage")]
pub mod storage;

pub use error::DataError;

pub use idempotency::{
    IdempotencyRecord, IdempotencyStatus, IdempotencyStore, InMemoryIdempotencyStore, Outcome,
};
pub use locks::{InMemoryLockManager, LockGuard, LockManager, LockToken};
pub use outbox::{InMemoryOutbox, Outbox, OutboxEntry, OutboxId};

#[cfg(feature = "sql")]
pub use outbox_sql::SqlOutbox;

#[cfg(feature = "redis")]
pub use locks_redis::RedisLockManager;

#[cfg(feature = "redis")]
pub use idempotency_redis::RedisIdempotencyStore;
