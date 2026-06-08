//! An in-memory config provider (programmatic / test config).

use std::collections::BTreeMap;

use serde_json::Value;

use super::{ConfigProvider, ProviderKind};
use crate::config::map::ConfigMap;
use crate::error::ConfigError;

/// A provider backed by an in-memory map.
///
/// Useful for three things:
/// * supplying built-in defaults at the bottom of a provider chain,
/// * programmatic overrides at the top of a chain,
/// * deterministic fixtures in tests without touching the filesystem, env, or
///   network.
#[derive(Debug, Default, Clone)]
pub struct MemoryProvider {
    values: BTreeMap<String, Value>,
}

impl MemoryProvider {
    /// An empty provider.
    pub fn new() -> Self {
        Self::default()
    }

    /// A provider seeded from an existing map.
    pub fn from_map(values: BTreeMap<String, Value>) -> Self {
        Self { values }
    }

    /// Builder-style insert of a single top-level key.
    pub fn set(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.values.insert(key.into(), value.into());
        self
    }
}

#[async_trait::async_trait]
impl ConfigProvider for MemoryProvider {
    fn name(&self) -> String {
        "memory".to_owned()
    }

    fn kind(&self) -> ProviderKind {
        ProviderKind::Memory
    }

    async fn load(&self) -> Result<ConfigMap, ConfigError> {
        Ok(ConfigMap::from(self.values.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn loads_seeded_values() {
        let provider = MemoryProvider::new()
            .set("app_name", "svc")
            .set("database", json!({ "url": "postgres://localhost" }));

        let out = provider.load().await.unwrap();
        assert_eq!(out.get("app_name"), Some(&json!("svc")));
        assert_eq!(out.get("database"), Some(&json!({ "url": "postgres://localhost" })));
    }
}
