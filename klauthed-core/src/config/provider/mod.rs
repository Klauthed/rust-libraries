use std::collections::BTreeMap;

use serde_json::Value;

pub mod env;
pub mod toml;
pub mod vault;
pub mod memory;

use crate::error::ConfigError;


#[async_trait::async_trait]
pub trait ConfigProvider {
    async fn load(&self) -> Result<BTreeMap<String, Value>, ConfigError>;
}
