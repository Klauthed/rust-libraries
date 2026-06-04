use std::{collections::BTreeMap, env};

use serde_json::Value;

use super::profile::Profile;
use crate::error::ConfigError;

/// The merged, resolved configuration tree.
pub struct Config {
    profile: Profile,
    values: BTreeMap<String, Value>,
}

impl Config {
    /// Create a new `Config` with the given `profile` and `values`.
    pub(crate) fn new(profile: Profile, values: BTreeMap<String, Value>) -> Self {
        Self { profile, values }
    }

    pub(crate) fn build() -> Self {
        let profile = Profile::detect();
        let values: BTreeMap<String, Value> = BTreeMap::new();
        Self { profile, values }
    }

    /// Return the active profile.
    pub fn profile(&self) -> &Profile {
        &self.profile
    }

    /// Return a reference to the raw config values. This is mostly for internal use and testing.
    pub fn values(&self) -> &BTreeMap<String, Value> {
        &self.values
    }

    /// Return an iterator over all config keys available in this config.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.values.keys().map(String::as_str)
    }

    pub fn build_values(&mut self, values: BTreeMap<String, Value>) -> BTreeMap<String, Value> {
        match self.profile {
            Profile::Local => {
                // Build values for local profile from files and env vars
            }
            Profile::Dev => {
                // Build values for dev profile from files and env vars
            }
            Profile::Test => {
                // Build values for test profile from files and env vars
            }
            Profile::Staging => {
                // Build values for staging profile from Vault
            }
            Profile::Prod => {
                // Build values for prod profile from Vault
            }   
        };

        values
    }
    
    /// Get a config value by key, deserializing it to the requested type T.
    /// Returns an error if the key is missing or if deserialization fails.
    /// For simple string values, consider using `get_string` instead to avoid unnecessary deserialization.
    /// 
    /// Example:
    /// ```
    /// struct DatabaseConfig {
    ///    url: String,
    ///    pool_size: u32,
    /// }
    /// let db_config: DatabaseConfig = config.get::<DatabaseConfig>("database").expect("Missing database config");
    /// ```
    pub fn get<T: serde::de::DeserializeOwned>(&self, key: &str) -> Result<T, ConfigError> {
        let value = self.get_raw(key).ok_or_else(|| ConfigError::MissingRequired(key.to_owned()))?;
        serde_json::from_value(value.clone()).map_err(|e| ConfigError::Deserialization {
            key: key.to_owned(),
            source: e,
        })
    }

    pub fn get_string(&self, key: &str) -> Option<String> {
        match self.get_raw(key)? {
            Value::String(s) => Some(s.clone()),
            other => Some(other.to_string()),
        }
    }

    pub fn get_raw(&self, key: &str) -> Option<&Value> {
        if let Some(v) = self.values.get(key) {
            return Some(v);
        }

        let (head, tail) = key.split_once('.')?;
        let parent = self.values.get(head)?;
        get_nested(parent, tail)
    }
}

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
    use serde_json::json;
    
    #[derive(serde::Deserialize, Debug, PartialEq)]
    struct DatabaseConfig {
        url: String,
        pool_size: u32,
    }

    fn create_test_config() -> Config {
        let mut values = BTreeMap::new();

        let db_details = json!({
            "url": "postgres://localhost",
            "pool_size": 10
        });
        
        values.insert("app_name".to_string(), Value::String("TestApp".to_string()));
        values.insert("debug".to_string(), Value::Bool(true));
        values.insert("database".to_string(), db_details);

        let config = Config::build();
        config
    }

    #[test]
    fn test_config_get() {        
        let config = create_test_config();
        let db_config: DatabaseConfig = config.get("database").expect("Missing database config");

        // panic!("   Got db_config: {:#?}", db_config);
        
        assert_eq!(db_config.url, "postgres://localhost");
        assert_eq!(db_config.pool_size, 10);
    }
}
