//! Wire a `CacheConfig` into a real, working in-memory cache.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p klauthed-data --example memory_cache --features cache-memory
//! ```
//!
//! This needs no external service — moka runs in-process — so it demonstrates
//! the config → connection flow end to end on any machine.

use klauthed_core::config::{CacheBackend, CacheConfig};
use klauthed_data::cache::build_memory_cache;

#[tokio::main]
async fn main() {
    // In a real service this comes from `Config::load().await?.cache()?`.
    let config = CacheConfig {
        backend: CacheBackend::InMemory,
        max_entries: 1_000,
        default_ttl_secs: 30,
        ..Default::default()
    };

    let cache = build_memory_cache::<String, String>(&config);

    cache.insert("greeting".into(), "hello".into()).await;
    cache.insert("subject".into(), "klauthed".into()).await;

    println!("greeting = {:?}", cache.get("greeting").await);
    println!("subject  = {:?}", cache.get("subject").await);
    println!("missing  = {:?}", cache.get("nope").await);
    println!("entries  ~ {}", cache.entry_count());
}
