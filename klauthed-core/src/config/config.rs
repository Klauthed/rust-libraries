//! The resolved `Config` and its typed-section accessors.

use std::collections::BTreeMap;

use serde_json::Value;

use super::builder::ConfigBuilder;
use super::keys;
use super::map::ConfigMap;
use super::profile::Profile;
use super::schema::{CacheConfig, DatabaseConfig, MessagingConfig, ServerConfig, StorageConfig};
use crate::error::ConfigError;

/// The merged, resolved configuration tree.
///
/// A `Config` is produced once at startup (typically via [`Config::load`] or
/// [`Config::builder`]) and read synchronously thereafter. Values are stored as
/// a nested JSON tree; typed sections are pulled out with [`get`](Self::get) or
/// the convenience accessors ([`database`](Self::database), …).
#[derive(Debug, Clone)]
pub struct Config {
    profile: Profile,
    values: ConfigMap,
}

impl Config {
    /// Construct directly from a resolved tree. Crate-internal; the public entry
    /// points are [`Config::builder`] and [`Config::load`].
    pub(crate) fn new(profile: Profile, values: ConfigMap) -> Self {
        Self { profile, values }
    }

    /// Start a [`ConfigBuilder`] for an explicit `profile`.
    pub fn builder(profile: Profile) -> ConfigBuilder {
        ConfigBuilder::new(profile)
    }

    /// Detect the profile from the environment and load using the conventional
    /// provider chain (see [`ConfigBuilder::with_defaults`]).
    ///
    /// Equivalent to `Config::builder(Profile::detect()).build().await` — the
    /// builder auto-applies the profile's default provider chain when none are
    /// registered.
    pub async fn load() -> Result<Self, ConfigError> {
        Self::builder(Profile::detect()).build().await
    }

    /// Alias for [`load`](Self::load): detect the profile and auto-load.
    pub async fn auto_load() -> Result<Self, ConfigError> {
        Self::load().await
    }

    /// The active profile.
    pub fn profile(&self) -> &Profile {
        &self.profile
    }

    /// The resolved tree — mostly for diagnostics and tests.
    pub fn values(&self) -> &ConfigMap {
        &self.values
    }

    /// The resolved tree as a plain `BTreeMap`.
    pub fn as_map(&self) -> &BTreeMap<String, Value> {
        self.values.as_map()
    }

    /// Iterate over the top-level config keys.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.values.keys()
    }

    /// Whether a (possibly dotted) key resolves to a value.
    pub fn contains_key(&self, key: &str) -> bool {
        self.get_raw(key).is_some()
    }

    /// Get and deserialize a config value by (possibly dotted) `key`.
    ///
    /// Returns [`ConfigError::MissingRequired`] if absent, or
    /// [`ConfigError::Deserialization`] if the shape does not match `T`.
    ///
    /// ```no_run
    /// # use klauthed_core::config::Config;
    /// # use serde::Deserialize;
    /// # async fn run(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    /// #[derive(Deserialize)]
    /// struct DatabaseConfig { url: String, pool_size: u32 }
    /// let db: DatabaseConfig = config.get("database")?;
    /// # let _ = db; Ok(())
    /// # }
    /// ```
    pub fn get<T: serde::de::DeserializeOwned>(&self, key: &str) -> Result<T, ConfigError> {
        let value =
            self.get_raw(key).ok_or_else(|| ConfigError::MissingRequired(key.to_owned()))?;
        serde_json::from_value(value.clone())
            .map_err(|e| ConfigError::Deserialization { key: key.to_owned(), source: e })
    }

    /// Like [`get`](Self::get) but returns `Ok(None)` when the key is absent.
    /// A present-but-malformed value still yields [`ConfigError::Deserialization`].
    pub fn get_optional<T: serde::de::DeserializeOwned>(
        &self,
        key: &str,
    ) -> Result<Option<T>, ConfigError> {
        match self.get_raw(key) {
            None => Ok(None),
            Some(value) => serde_json::from_value(value.clone())
                .map(Some)
                .map_err(|e| ConfigError::Deserialization { key: key.to_owned(), source: e }),
        }
    }

    /// Get a value as a string. Non-string scalars are rendered via their JSON
    /// representation; objects/arrays return their JSON text.
    pub fn get_string(&self, key: &str) -> Option<String> {
        match self.get_raw(key)? {
            Value::String(s) => Some(s.clone()),
            other => Some(other.to_string()),
        }
    }

    /// Resolve a (possibly dotted) key to a raw [`Value`], walking nested objects.
    pub fn get_raw(&self, key: &str) -> Option<&Value> {
        if let Some(v) = self.values.get(key) {
            return Some(v);
        }
        let (head, tail) = key.split_once('.')?;
        let parent = self.values.get(head)?;
        get_nested(parent, tail)
    }

    // ── Typed-section convenience accessors ───────────────────────────────────
    //
    // Thin wrappers over `get` using the conventional section keys, so services
    // can write `config.database()?` instead of `config.get("database")?`.

    /// The `database` section.
    pub fn database(&self) -> Result<DatabaseConfig, ConfigError> {
        self.get(keys::DATABASE)
    }

    /// The `cache` section.
    pub fn cache(&self) -> Result<CacheConfig, ConfigError> {
        self.get(keys::CACHE)
    }

    /// The `messaging` section (broker-agnostic: NATS / RabbitMQ / Kafka).
    pub fn messaging(&self) -> Result<MessagingConfig, ConfigError> {
        self.get(keys::MESSAGING)
    }

    /// The `storage` section.
    pub fn storage(&self) -> Result<StorageConfig, ConfigError> {
        self.get(keys::STORAGE)
    }

    /// The `server` section.
    pub fn server(&self) -> Result<ServerConfig, ConfigError> {
        self.get(keys::SERVER)
    }
}

/// Walk a nested object along a dotted `path`.
fn get_nested<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut parts = path.splitn(2, '.');
    let head = parts.next()?;
    let child = value.get(head)?;
    match parts.next() {
        Some(tail) => get_nested(child, tail),
        None => Some(child),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::provider::MemoryProvider;
    use serde::Deserialize;
    use serde_json::json;

    #[derive(Deserialize, Debug, PartialEq)]
    struct DbFixture {
        url: String,
        pool_size: u32,
    }

    async fn fixture() -> Config {
        Config::builder(Profile::Test)
            .with_provider(
                MemoryProvider::new()
                    .set("app_name", "TestApp")
                    .set("debug", true)
                    .set("database", json!({ "url": "postgres://localhost", "pool_size": 10 })),
            )
            .build()
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn get_deserializes_nested_section() {
        let config = fixture().await;
        let db: DbFixture = config.get("database").unwrap();
        assert_eq!(db, DbFixture { url: "postgres://localhost".into(), pool_size: 10 });
    }

    #[tokio::test]
    async fn dotted_keys_resolve_into_nested_objects() {
        let config = fixture().await;
        assert_eq!(config.get_raw("database.url"), Some(&json!("postgres://localhost")));
        assert_eq!(config.get_string("database.pool_size").as_deref(), Some("10"));
    }

    #[tokio::test]
    async fn missing_vs_optional() {
        let config = fixture().await;
        assert!(matches!(config.get::<DbFixture>("nope"), Err(ConfigError::MissingRequired(_))));
        assert_eq!(config.get_optional::<DbFixture>("nope").unwrap(), None);
        assert!(!config.contains_key("nope"));
        assert!(config.contains_key("database.url"));
    }
}
