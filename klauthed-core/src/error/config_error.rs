use klauthed_error::{DomainError, ErrorCategory, ErrorCode};
use thiserror::Error;

/// Errors produced while loading, merging, or reading configuration.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ConfigError {
    #[error("config file not found: {0}")]
    FileNotFound(std::path::PathBuf),

    #[error("unsupported config file format for '{0}' (expected .toml or .json)")]
    UnsupportedFormat(std::path::PathBuf),

    #[error("config parse error in '{path}': {message}")]
    ParseError { path: String, message: String },

    #[error("required config key is missing: {0}")]
    MissingRequired(String),

    #[error(
        "secret '{key}' must be sourced from Vault in profile '{profile}', \
         not from files or environment"
    )]
    VaultRequired { key: String, profile: String },

    #[error("invalid provider chain for profile '{profile}': {message}")]
    InvalidProviderChain { profile: String, message: String },

    #[error("deserialization error for key '{key}': {source}")]
    Deserialization { key: String, source: serde_json::Error },

    // ── Vault provider (feature = "vault") ────────────────────────────────────
    #[error("vault authentication failed via {method}: {message}")]
    VaultAuth { method: String, message: String },

    #[error("vault request to '{path}' failed: {message}")]
    VaultRequest { path: String, message: String },

    #[error("vault secret not found at path: {0}")]
    VaultSecretNotFound(String),

    #[error("missing required environment variable: {0}")]
    MissingEnv(String),

    // ── Wrapped sources ───────────────────────────────────────────────────────
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML error: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[cfg(feature = "vault")]
    #[error("vault HTTP transport error: {0}")]
    Http(#[from] reqwest::Error),
}

impl ConfigError {
    /// Map a variant to its category and stable code in one place.
    fn classify(&self) -> (ErrorCategory, &'static str) {
        use ErrorCategory::{Internal, Unavailable};
        match self {
            ConfigError::FileNotFound(_) => (Internal, "config.file_not_found"),
            ConfigError::UnsupportedFormat(_) => (Internal, "config.unsupported_format"),
            ConfigError::ParseError { .. } => (Internal, "config.parse_error"),
            ConfigError::MissingRequired(_) => (Internal, "config.missing_required"),
            ConfigError::VaultRequired { .. } => (Internal, "config.vault_required"),
            ConfigError::InvalidProviderChain { .. } => (Internal, "config.invalid_provider_chain"),
            ConfigError::Deserialization { .. } => (Internal, "config.deserialization"),
            ConfigError::VaultAuth { .. } => (Internal, "config.vault_auth"),
            // Vault reachable-but-failed is transient from the service's view.
            ConfigError::VaultRequest { .. } => (Unavailable, "config.vault_request"),
            ConfigError::VaultSecretNotFound(_) => (Internal, "config.vault_secret_not_found"),
            ConfigError::MissingEnv(_) => (Internal, "config.missing_env"),
            ConfigError::Io(_) => (Internal, "config.io"),
            ConfigError::Toml(_) => (Internal, "config.toml"),
            ConfigError::Json(_) => (Internal, "config.json"),
            #[cfg(feature = "vault")]
            ConfigError::Http(_) => (Unavailable, "config.vault_http"),
        }
    }
}

impl DomainError for ConfigError {
    fn category(&self) -> ErrorCategory {
        self.classify().0
    }

    fn code(&self) -> ErrorCode {
        ErrorCode::new(self.classify().1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_config_errors() {
        let missing = ConfigError::MissingRequired("database".into());
        assert_eq!(missing.category(), ErrorCategory::Internal);
        assert_eq!(missing.code().as_str(), "config.missing_required");
        assert_eq!(missing.http_status(), 500);
        assert!(!missing.is_retryable());

        let vault = ConfigError::VaultRequest { path: "secret/app".into(), message: "503".into() };
        assert_eq!(vault.category(), ErrorCategory::Unavailable);
        assert!(vault.is_retryable());
    }
}
