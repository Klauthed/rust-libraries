//! [`ConfigSource`] — where the config server reads the configuration it serves.

use std::collections::HashMap;
use std::path::PathBuf;

use async_trait::async_trait;
use klauthed_core::config::ConfigMap;
use klauthed_core::config::provider::{ConfigProvider, FileProvider};

/// An error raised by a [`ConfigSource`] backend.
#[derive(Debug)]
#[non_exhaustive]
pub enum ConfigSourceError {
    /// A backing store could not be read or parsed.
    Backend(String),
}

impl std::fmt::Display for ConfigSourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigSourceError::Backend(msg) => write!(f, "config source error: {msg}"),
        }
    }
}

impl std::error::Error for ConfigSourceError {}

/// Resolves the configuration a [`ConfigServer`](super::ConfigServer) serves for
/// a given application/profile.
///
/// An unknown application/profile yields an **empty** map, not an error — callers
/// decide whether missing config is fatal.
#[async_trait]
pub trait ConfigSource: Send + Sync + 'static {
    /// The merged configuration tree for `application` + `profile` (+ optional
    /// `label`).
    ///
    /// # Errors
    /// Returns [`ConfigSourceError`] only on a backend failure (I/O, parse).
    async fn fetch(
        &self,
        application: &str,
        profile: &str,
        label: Option<&str>,
    ) -> Result<ConfigMap, ConfigSourceError>;
}

/// A [`ConfigSource`] backed by a directory of TOML/JSON files.
///
/// For `application = "auth-api"`, `profile = "prod"`, files are layered (later
/// wins): `application.{toml,json}` (shared defaults) → `auth-api.{toml,json}`
/// (app defaults) → `auth-api-prod.{toml,json}` (app + profile). Missing files
/// are skipped; `label` is currently ignored.
#[derive(Debug, Clone)]
pub struct DirectoryConfigSource {
    root: PathBuf,
}

impl DirectoryConfigSource {
    /// Serve configuration from the directory `root`.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

#[async_trait]
impl ConfigSource for DirectoryConfigSource {
    async fn fetch(
        &self,
        application: &str,
        profile: &str,
        _label: Option<&str>,
    ) -> Result<ConfigMap, ConfigSourceError> {
        let stems =
            ["application".to_owned(), application.to_owned(), format!("{application}-{profile}")];

        let mut merged = ConfigMap::new();
        for stem in stems {
            for ext in ["toml", "json"] {
                let path = self.root.join(format!("{stem}.{ext}"));
                if path.is_file() {
                    let loaded = FileProvider::new(path)
                        .load()
                        .await
                        .map_err(|e| ConfigSourceError::Backend(e.to_string()))?;
                    merged.merge(loaded);
                }
            }
        }
        Ok(merged)
    }
}

/// An in-memory [`ConfigSource`] keyed by `(application, profile)` — for tests
/// and small/static deployments.
#[derive(Debug, Default, Clone)]
pub struct InMemoryConfigSource {
    entries: HashMap<(String, String), ConfigMap>,
}

impl InMemoryConfigSource {
    /// An empty source.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the configuration served for `application`/`profile` (builder form).
    #[must_use]
    pub fn with(
        mut self,
        application: impl Into<String>,
        profile: impl Into<String>,
        config: ConfigMap,
    ) -> Self {
        self.entries.insert((application.into(), profile.into()), config);
        self
    }
}

#[async_trait]
impl ConfigSource for InMemoryConfigSource {
    async fn fetch(
        &self,
        application: &str,
        profile: &str,
        _label: Option<&str>,
    ) -> Result<ConfigMap, ConfigSourceError> {
        Ok(self
            .entries
            .get(&(application.to_owned(), profile.to_owned()))
            .cloned()
            .unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[actix_web::test]
    async fn in_memory_returns_set_config_and_empty_for_unknown() {
        let source = InMemoryConfigSource::new().with(
            "auth-api",
            "prod",
            ConfigMap::from_iter([("port".to_owned(), json!(8443))]),
        );

        let found = source.fetch("auth-api", "prod", None).await.unwrap();
        assert_eq!(found.get("port"), Some(&json!(8443)));

        let missing = source.fetch("auth-api", "dev", None).await.unwrap();
        assert!(missing.is_empty());
    }

    #[actix_web::test]
    async fn directory_layers_shared_app_and_profile_files() {
        let dir = std::env::temp_dir().join(format!("klauthed_cfgsrc_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("application.toml"), "shared = true\nport = 80\n").unwrap();
        std::fs::write(dir.join("auth-api-prod.toml"), "port = 8443\n").unwrap();

        let source = DirectoryConfigSource::new(&dir);
        let config = source.fetch("auth-api", "prod", None).await.unwrap();

        // Shared default carried over; profile file overrode `port`.
        assert_eq!(config.get("shared"), Some(&json!(true)));
        assert_eq!(config.get("port"), Some(&json!(8443)));

        std::fs::remove_dir_all(&dir).ok();
    }
}
