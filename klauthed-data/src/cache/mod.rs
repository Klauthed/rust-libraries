//! Cache connections from a [`CacheConfig`](klauthed_core::config::CacheConfig).
//!
//! * `redis` feature → [`connect_redis`], a managed async Redis connection.
//! * `cache-memory` feature → `build_memory_cache`, an in-process moka cache.

#[cfg(feature = "redis")]
pub mod redis;

#[cfg(feature = "cache-memory")]
pub mod memory;

#[cfg(feature = "redis")]
pub use redis::connect as connect_redis;

#[cfg(feature = "cache-memory")]
pub use memory::build_memory_cache;
