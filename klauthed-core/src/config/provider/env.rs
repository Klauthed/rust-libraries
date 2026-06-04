use std::collections::BTreeMap;

use serde_json::Value;

use crate::config::ConfigProvider;
use crate::error::ConfigError;

pub struct EnvProvider;

#[async_trait::async_trait]
impl ConfigProvider for EnvProvider {
    async fn load(&self) -> Result<BTreeMap<String, Value>, ConfigError> {
        let mut values = BTreeMap::new();

        for (key, value) in std::env::vars() {
            values.insert(
                key.to_lowercase().replace("__", "."),
                Value::String(value),
            );
        }

        Ok(values)
    }
}
