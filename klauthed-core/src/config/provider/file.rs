use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{ConfigProvider, ProviderKind};
use crate::config::map::ConfigMap;
use crate::error::ConfigError;

/// Supported on-disk config formats, selected by file extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Toml,
    Json,
}

impl Format {
    fn from_path(path: &Path) -> Option<Self> {
        match path.extension().and_then(|e| e.to_str()) {
            Some("toml") => Some(Format::Toml),
            Some("json") => Some(Format::Json),
            _ => None,
        }
    }
}

/// Reads configuration from a `.toml` or `.json` file on disk.
///
/// File contents are naturally nested, so they map directly onto the config
/// tree. A provider may be marked *optional*, in which case a missing file
/// contributes nothing instead of failing — this is what lets a profile layer
/// (`config/{profile}.toml`) be absent without breaking startup.
pub struct FileProvider {
    path: PathBuf,
    optional: bool,
}

impl FileProvider {
    /// A required file. Loading fails if the file is missing.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into(), optional: false }
    }

    /// An optional file. A missing file contributes an empty map.
    pub fn optional(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into(), optional: true }
    }

    fn parse(&self, text: &str, format: Format) -> Result<ConfigMap, ConfigError> {
        let value: Value = match format {
            Format::Toml => toml::from_str(text)?,
            Format::Json => serde_json::from_str(text)?,
        };

        match value {
            Value::Object(map) => Ok(map.into_iter().collect()),
            _ => Err(ConfigError::ParseError {
                path: self.path.display().to_string(),
                message: "top-level config must be a table/object".to_owned(),
            }),
        }
    }
}

#[async_trait::async_trait]
impl ConfigProvider for FileProvider {
    fn name(&self) -> String {
        format!("file:{}", self.path.display())
    }

    fn kind(&self) -> ProviderKind {
        ProviderKind::File
    }

    async fn load(&self) -> Result<ConfigMap, ConfigError> {
        let format = Format::from_path(&self.path)
            .ok_or_else(|| ConfigError::UnsupportedFormat(self.path.clone()))?;

        let text = match std::fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                if self.optional {
                    tracing::debug!(path = %self.path.display(), "optional config file absent; skipping");
                    return Ok(ConfigMap::new());
                }
                return Err(ConfigError::FileNotFound(self.path.clone()));
            }
            Err(err) => return Err(ConfigError::Io(err)),
        };

        self.parse(&text, format)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_toml_into_nested_tree() {
        let provider = FileProvider::new("x.toml");
        let out = provider
            .parse(
                r#"
                app_name = "svc"
                [database]
                url = "postgres://localhost"
                pool_size = 10
                "#,
                Format::Toml,
            )
            .unwrap();

        assert_eq!(out.get("app_name"), Some(&json!("svc")));
        assert_eq!(
            out.get("database"),
            Some(&json!({ "url": "postgres://localhost", "pool_size": 10 }))
        );
    }

    #[test]
    fn parses_json_into_nested_tree() {
        let provider = FileProvider::new("x.json");
        let out =
            provider.parse(r#"{ "debug": true, "cache": { "ttl": 30 } }"#, Format::Json).unwrap();

        assert_eq!(out.get("debug"), Some(&json!(true)));
        assert_eq!(out.get("cache"), Some(&json!({ "ttl": 30 })));
    }

    #[test]
    fn rejects_non_table_top_level() {
        let provider = FileProvider::new("x.json");
        let err = provider.parse("[1, 2, 3]", Format::Json).unwrap_err();
        assert!(matches!(err, ConfigError::ParseError { .. }));
    }

    #[tokio::test]
    async fn optional_missing_file_yields_empty() {
        let provider = FileProvider::optional("does/not/exist.toml");
        let out = provider.load().await.unwrap();
        assert!(out.is_empty());
    }

    #[tokio::test]
    async fn required_missing_file_errors() {
        let provider = FileProvider::new("does/not/exist.toml");
        let err = provider.load().await.unwrap_err();
        assert!(matches!(err, ConfigError::FileNotFound(_)));
    }

    #[tokio::test]
    async fn unsupported_extension_errors() {
        let provider = FileProvider::new("config.yaml");
        let err = provider.load().await.unwrap_err();
        assert!(matches!(err, ConfigError::UnsupportedFormat(_)));
    }
}
