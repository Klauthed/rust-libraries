//! Managed async Redis connection from a [`CacheConfig`].

use klauthed_core::config::{CacheBackend, CacheConfig};

use crate::error::DataError;

/// Open a managed Redis connection (auto-reconnecting) from `config`.
///
/// Returns [`DataError::UnsupportedCacheBackend`] if the config selects the
/// in-memory backend rather than Redis.
pub async fn connect(config: &CacheConfig) -> Result<::redis::aio::ConnectionManager, DataError> {
    if config.backend != CacheBackend::Redis {
        return Err(DataError::UnsupportedCacheBackend(config.backend));
    }

    let url = config
        .connection_url()
        .ok_or(DataError::MissingUrl("redis"))?;

    tracing::debug!("connecting to redis cache");
    let client = ::redis::Client::open(url)?;
    let manager = ::redis::aio::ConnectionManager::new(client).await?;
    Ok(manager)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rejects_in_memory_backend() {
        let config = CacheConfig {
            backend: CacheBackend::InMemory,
            ..Default::default()
        };
        let err = connect(&config).await.unwrap_err();
        assert!(matches!(
            err,
            DataError::UnsupportedCacheBackend(CacheBackend::InMemory)
        ));
    }
}
