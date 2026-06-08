//! [`FromConfig`] — bind a typed struct to a section of the resolved [`Config`].
//!
//! The hand-written typed sections ([`DatabaseConfig`](super::DatabaseConfig),
//! …) are read with the convenience accessors; `FromConfig` is the same idea for
//! *your* config structs. Derive it (the
//! [`#[derive(FromConfig)]`](macro@FromConfig) macro) to bind a struct to a
//! config key without hand-writing the `config.get(...)` call — the klauthed
//! analog of Spring's `@ConfigurationProperties`.
//!
//! ```
//! use klauthed_core::config::{Config, ConfigBuilder, FromConfig, Profile};
//! use klauthed_core::config::provider::MemoryProvider;
//! use serde::Deserialize;
//! use serde_json::json;
//!
//! #[derive(Debug, Deserialize, FromConfig)]
//! #[config(key = "database")]
//! struct Db {
//!     host: String,
//!     port: u16,
//! }
//!
//! # async fn run() -> Result<(), klauthed_core::error::ConfigError> {
//! let config = ConfigBuilder::new(Profile::Test)
//!     .with_provider(MemoryProvider::new().set("database", json!({ "host": "db", "port": 5432 })))
//!     .build()
//!     .await?;
//!
//! let db = Db::from_config(&config)?;
//! assert_eq!(db.host, "db");
//! # Ok(())
//! # }
//! ```

use crate::config::Config;
use crate::error::ConfigError;

/// A type that can be read from a section of the resolved [`Config`].
///
/// Usually derived with [`#[derive(FromConfig)]`](macro@FromConfig); implement
/// it by hand only for bespoke binding logic.
pub trait FromConfig: Sized {
    /// Read and deserialize this type from `config`.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] if the bound section is missing (unless the field
    /// is defaulted) or its shape does not match.
    fn from_config(config: &Config) -> Result<Self, ConfigError>;
}

/// Derive [`FromConfig`] for a struct, binding it to a config key.
///
/// ```text
/// #[derive(serde::Deserialize, FromConfig)]
/// #[config(key = "database")]      // defaults to the snake_cased type name
/// struct DatabaseSettings { /* … */ }
///
/// #[derive(Default, serde::Deserialize, FromConfig)]
/// #[config(key = "cache", default)]  // missing section → `Default::default()`
/// struct CacheSettings { /* … */ }
/// ```
///
/// The type must also implement [`serde::Deserialize`]; with `default`, it must
/// implement [`Default`].
pub use klauthed_macros::FromConfig;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::provider::MemoryProvider;
    use crate::config::{ConfigBuilder, Profile};
    use crate::error::ConfigError;
    use serde::Deserialize;
    use serde_json::json;

    #[derive(Debug, PartialEq, Eq, Deserialize, FromConfig)]
    #[config(key = "database")]
    struct Database {
        host: String,
        port: u16,
    }

    // No explicit key → binds to the snake_cased type name ("server_settings").
    #[derive(Debug, PartialEq, Eq, Deserialize, FromConfig)]
    struct ServerSettings {
        workers: u32,
    }

    #[derive(Debug, Default, PartialEq, Eq, Deserialize, FromConfig)]
    #[config(key = "cache", default)]
    struct Cache {
        ttl_secs: u64,
    }

    async fn config_with(key: &str, value: serde_json::Value) -> Config {
        ConfigBuilder::new(Profile::Test)
            .with_provider(MemoryProvider::new().set(key, value))
            .build()
            .await
            .expect("build config")
    }

    #[tokio::test]
    async fn binds_explicit_key() {
        let config = config_with("database", json!({ "host": "db", "port": 5432 })).await;
        assert_eq!(
            Database::from_config(&config).unwrap(),
            Database { host: "db".to_owned(), port: 5432 }
        );
    }

    #[tokio::test]
    async fn defaults_key_to_snake_cased_type_name() {
        let config = config_with("server_settings", json!({ "workers": 8 })).await;
        assert_eq!(ServerSettings::from_config(&config).unwrap(), ServerSettings { workers: 8 });
    }

    #[tokio::test]
    async fn default_mode_binds_missing_section_to_default() {
        let config = config_with("unrelated", json!(true)).await;
        assert_eq!(Cache::from_config(&config).unwrap(), Cache { ttl_secs: 0 });
    }

    #[tokio::test]
    async fn missing_required_section_errors() {
        let config = config_with("unrelated", json!(true)).await;
        assert!(matches!(Database::from_config(&config), Err(ConfigError::MissingRequired(_))));
    }
}
