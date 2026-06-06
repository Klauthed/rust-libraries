use serde::{Deserialize, Serialize};

use super::database::PoolConfig;

/// Which cache backend a service uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CacheBackend {
    /// Networked Redis (or a Redis-compatible server).
    #[default]
    Redis,
    /// Process-local in-memory cache (e.g. moka). No connection details apply.
    InMemory,
}

/// Cache connection and behavior.
///
/// For Redis, either set `url` or the component fields. For the in-memory
/// backend only `default_ttl_secs` / `max_entries` are meaningful.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheConfig {
    #[serde(default)]
    pub backend: CacheBackend,
    /// Full Redis URL; when set it overrides the component fields.
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default = "default_cache_host")]
    pub host: String,
    #[serde(default = "default_redis_port")]
    pub port: u16,
    /// Redis logical database index.
    #[serde(default)]
    pub db: u32,
    #[serde(default)]
    pub username: Option<String>,
    /// Password. Prefer sourcing this from Vault in staging/prod.
    #[serde(default)]
    pub password: Option<String>,
    /// Use TLS (`rediss://`).
    #[serde(default)]
    pub tls: bool,
    /// Default entry TTL in seconds.
    #[serde(default = "default_ttl_secs")]
    pub default_ttl_secs: u64,
    /// Maximum number of entries for the in-memory backend.
    #[serde(default = "default_max_entries")]
    pub max_entries: u64,
    /// Pool tuning (Redis backend).
    #[serde(default)]
    pub pool: PoolConfig,
}

fn default_cache_host() -> String {
    "localhost".to_owned()
}
fn default_redis_port() -> u16 {
    6379
}
fn default_ttl_secs() -> u64 {
    300
}
fn default_max_entries() -> u64 {
    10_000
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            backend: CacheBackend::default(),
            url: None,
            host: default_cache_host(),
            port: default_redis_port(),
            db: 0,
            username: None,
            password: None,
            tls: false,
            default_ttl_secs: default_ttl_secs(),
            max_entries: default_max_entries(),
            pool: PoolConfig::default(),
        }
    }
}

impl CacheConfig {
    /// Build a Redis connection URL from the components, or return `url`
    /// verbatim if set. Returns `None` for the in-memory backend.
    pub fn connection_url(&self) -> Option<String> {
        if self.backend == CacheBackend::InMemory {
            return None;
        }
        if let Some(url) = &self.url {
            return Some(url.clone());
        }

        let scheme = if self.tls { "rediss" } else { "redis" };
        let mut url = format!("{scheme}://");
        match (&self.username, &self.password) {
            (Some(user), Some(pass)) => {
                url.push_str(user);
                url.push(':');
                url.push_str(pass);
                url.push('@');
            }
            (None, Some(pass)) => {
                url.push(':');
                url.push_str(pass);
                url.push('@');
            }
            _ => {}
        }
        url.push_str(&self.host);
        url.push(':');
        url.push_str(&self.port.to_string());
        url.push('/');
        url.push_str(&self.db.to_string());
        Some(url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn redis_defaults_and_url() {
        let cfg: CacheConfig = serde_json::from_value(json!({})).unwrap();
        assert_eq!(cfg.backend, CacheBackend::Redis);
        assert_eq!(cfg.port, 6379);
        assert_eq!(cfg.default_ttl_secs, 300);
        assert_eq!(cfg.connection_url().as_deref(), Some("redis://localhost:6379/0"));
    }

    #[test]
    fn redis_with_password_and_tls() {
        let cfg =
            CacheConfig { password: Some("pw".into()), tls: true, db: 2, ..Default::default() };
        assert_eq!(cfg.connection_url().as_deref(), Some("rediss://:pw@localhost:6379/2"));
    }

    #[test]
    fn in_memory_has_no_url() {
        let cfg = CacheConfig { backend: CacheBackend::InMemory, ..Default::default() };
        assert_eq!(cfg.connection_url(), None);
    }
}
