use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config file not found: {0}")]
    FileNotFound(std::path::PathBuf),

    #[error("config parse error in '{path}': {message}")]
    ParseError { path: String, message: String },

    #[error("required config key is missing: {0}")]
    MissingRequired(String),

    #[error("secret '{key}' must be sourced from Vault in profile '{profile}', not from files or environment")]
    VaultRequired { key: String, profile: String },

    #[error("deserialization error for key '{key}': {source}")]
    Deserialization { key: String, source: serde_json::Error },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML error: {0}")]
    Toml(#[from] toml::de::Error),
}
