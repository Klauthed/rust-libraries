//! The `ConfigError` type for the config layer.

use klauthed_error::{DomainError, ErrorCategory, ErrorCode};
use thiserror::Error;

/// Errors produced while loading, merging, or reading configuration.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ConfigError {
    /// A referenced config file does not exist.
    #[error("config file not found: {0}")]
    FileNotFound(std::path::PathBuf),

    /// A config file's extension is neither `.toml` nor `.json`.
    #[error("unsupported config file format for '{0}' (expected .toml or .json)")]
    UnsupportedFormat(std::path::PathBuf),

    /// A config file could not be parsed.
    #[error("config parse error in '{path}': {message}")]
    ParseError {
        /// Path of the file that failed to parse.
        path: String,
        /// The underlying parser message.
        message: String,
    },

    /// A required configuration key was absent.
    #[error("required config key is missing: {0}")]
    MissingRequired(String),

    /// A secret must be Vault-sourced in this profile but came from elsewhere.
    #[error(
        "secret '{key}' must be sourced from Vault in profile '{profile}', \
         not from files or environment"
    )]
    VaultRequired {
        /// The secret key that must be Vault-sourced.
        key: String,
        /// The active profile enforcing the requirement.
        profile: String,
    },

    /// The configured provider chain is invalid for the profile.
    #[error("invalid provider chain for profile '{profile}': {message}")]
    InvalidProviderChain {
        /// The profile whose provider chain is invalid.
        profile: String,
        /// What is wrong with the chain.
        message: String,
    },

    /// A value could not be deserialized into the requested type.
    #[error("deserialization error for key '{key}': {source}")]
    Deserialization {
        /// The config key being deserialized.
        key: String,
        /// The underlying serde error.
        source: serde_json::Error,
    },

    // ── Vault provider (feature = "vault") ────────────────────────────────────
    /// Authentication to Vault failed.
    #[error("vault authentication failed via {method}: {message}")]
    VaultAuth {
        /// The auth method attempted (token / AppRole / Kubernetes).
        method: String,
        /// The failure detail.
        message: String,
    },

    /// A Vault request failed (reachable but errored) — typically transient.
    #[error("vault request to '{path}' failed: {message}")]
    VaultRequest {
        /// The Vault path requested.
        path: String,
        /// The failure detail.
        message: String,
    },

    /// No secret exists at the given Vault path.
    #[error("vault secret not found at path: {0}")]
    VaultSecretNotFound(String),

    /// A required environment variable was not set.
    #[error("missing required environment variable: {0}")]
    MissingEnv(String),

    // ── Wrapped sources ───────────────────────────────────────────────────────
    /// An underlying I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// A TOML deserialization error.
    #[error("TOML error: {0}")]
    Toml(#[from] toml::de::Error),

    /// A JSON deserialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// A Vault HTTP transport error (`vault` feature).
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
