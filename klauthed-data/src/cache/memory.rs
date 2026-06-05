//! In-process cache (moka) sized and TTL'd from a [`CacheConfig`].

use std::hash::Hash;
use std::time::Duration;

use klauthed_core::config::CacheConfig;
use moka::future::Cache;

/// Build an in-memory cache whose capacity and default TTL come from `config`
/// (`max_entries` and `default_ttl_secs`).
///
/// The backend field is not enforced here — this is the in-memory path by
/// construction — so it can also back a Redis-configured service in tests or
/// as a local fallback. The key/value types are chosen by the caller.
pub fn build_memory_cache<K, V>(config: &CacheConfig) -> Cache<K, V>
where
    K: Hash + Eq + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    Cache::builder()
        .max_capacity(config.max_entries)
        .time_to_live(Duration::from_secs(config.default_ttl_secs))
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn builds_and_stores_values() {
        let config = CacheConfig {
            max_entries: 100,
            default_ttl_secs: 60,
            ..Default::default()
        };
        let cache: Cache<String, u32> = build_memory_cache(&config);

        cache.insert("answer".to_string(), 42).await;
        assert_eq!(cache.get("answer").await, Some(42));
    }
}
