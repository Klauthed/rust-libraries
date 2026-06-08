//! Orchestration: assemble a provider chain, enforce profile policy, load and
//! merge into a resolved [`Config`].

use super::config::Config;
use super::map::ConfigMap;
use super::profile::Profile;
use super::provider::{ConfigProvider, ProviderKind};
use crate::error::ConfigError;

/// Builds a [`Config`] from an ordered chain of [`ConfigProvider`]s.
///
/// Providers are loaded in insertion order and deep-merged, so a provider added
/// later overrides earlier ones key-by-key. The conventional precedence (low →
/// high) is: built-in defaults → base file → profile file → Vault → environment.
///
/// At [`build`](Self::build) time the builder enforces the active profile's
/// policy — profiles that [require Vault](Profile::requires_vault) must include a
/// Vault provider and must not include file-secret providers.
pub struct ConfigBuilder {
    profile: Profile,
    providers: Vec<Box<dyn ConfigProvider>>,
}

impl ConfigBuilder {
    /// Start an empty builder for `profile`.
    pub fn new(profile: Profile) -> Self {
        Self { profile, providers: Vec::new() }
    }

    /// The profile this builder targets.
    pub fn profile(&self) -> &Profile {
        &self.profile
    }

    /// Append a provider to the chain (higher precedence than those before it).
    pub fn with_provider<P>(mut self, provider: P) -> Self
    where
        P: ConfigProvider + 'static,
    {
        self.providers.push(Box::new(provider));
        self
    }

    /// Append a boxed provider — handy when the concrete type is only known at
    /// runtime (e.g. conditionally a Vault provider).
    pub fn with_boxed_provider(mut self, provider: Box<dyn ConfigProvider>) -> Self {
        self.providers.push(provider);
        self
    }

    /// Populate the conventional provider chain for the active profile.
    ///
    /// * File-secret profiles (local/dev/test): optional `config/default.toml`,
    ///   optional `config/{profile}.toml`, then environment.
    /// * Vault profiles (staging/prod): Vault (from environment) then
    ///   environment. Requires the `vault` feature; without it, [`build`](Self::build)
    ///   will report a missing Vault provider.
    pub fn with_defaults(self) -> Result<Self, ConfigError> {
        use super::provider::{EnvProvider, FileProvider};

        if self.profile.allows_file_secrets() {
            let profile_file = format!("config/{}.toml", self.profile.as_str());
            Ok(self
                .with_provider(FileProvider::optional("config/default.toml"))
                .with_provider(FileProvider::optional(profile_file))
                .with_provider(EnvProvider::new()))
        } else {
            let builder = self.add_default_vault_provider()?;
            Ok(builder.with_provider(EnvProvider::new()))
        }
    }

    #[cfg(feature = "vault")]
    fn add_default_vault_provider(self) -> Result<Self, ConfigError> {
        use super::provider::vault::VaultProvider;
        Ok(self.with_provider(VaultProvider::from_env()?))
    }

    #[cfg(not(feature = "vault"))]
    fn add_default_vault_provider(self) -> Result<Self, ConfigError> {
        // Without the `vault` feature there is no provider to add; build() will
        // reject the chain with a clear "requires Vault" error for this profile.
        Ok(self)
    }

    /// Validate the chain against profile policy without performing any I/O.
    fn enforce_policy(&self) -> Result<(), ConfigError> {
        if !self.profile.requires_vault() {
            return Ok(());
        }

        if let Some(file) = self.providers.iter().find(|p| p.kind().is_file_secret_source()) {
            return Err(ConfigError::InvalidProviderChain {
                profile: self.profile.to_string(),
                message: format!(
                    "provider '{}' sources secrets from files, which is not allowed; \
                     this profile must use Vault",
                    file.name()
                ),
            });
        }

        let has_vault = self.providers.iter().any(|p| p.kind() == ProviderKind::Vault);
        if !has_vault {
            return Err(ConfigError::InvalidProviderChain {
                profile: self.profile.to_string(),
                message: "this profile requires a Vault provider but none was configured \
                          (enable the `vault` feature and set VAULT_ADDR, or add one explicitly)"
                    .into(),
            });
        }

        Ok(())
    }

    /// Apply the conventional default provider chain when none were registered,
    /// otherwise return the builder unchanged. Mirrors what [`build`](Self::build)
    /// does internally; exposed so [`ReloadableConfig`](super::ReloadableConfig)
    /// can prepare the chain once before re-resolving it repeatedly.
    ///
    /// # Errors
    /// Returns [`ConfigError`] if the default chain cannot be assembled.
    pub fn ensure_defaults(self) -> Result<Self, ConfigError> {
        if self.providers.is_empty() { self.with_defaults() } else { Ok(self) }
    }

    /// Enforce profile policy, then load every provider in order and deep-merge
    /// their output into a resolved [`Config`] — **without consuming** the
    /// builder, so the same chain can be re-resolved (this is what powers
    /// hot-reload).
    ///
    /// Unlike [`build`](Self::build) it does not auto-apply the default chain for
    /// an empty builder; call [`ensure_defaults`](Self::ensure_defaults) or
    /// register providers first.
    ///
    /// # Errors
    /// Returns [`ConfigError`] on a profile-policy violation or a provider load
    /// failure.
    pub async fn resolve(&self) -> Result<Config, ConfigError> {
        self.enforce_policy()?;

        let mut acc = ConfigMap::new();
        for provider in &self.providers {
            let loaded = provider.load().await?;
            tracing::debug!(provider = %provider.name(), keys = loaded.len(), "loaded config provider");
            acc.merge(loaded);
        }

        Ok(Config::new(self.profile.clone(), acc))
    }

    /// Load every provider in order, deep-merge their output, and produce a
    /// resolved [`Config`].
    ///
    /// If no providers were registered, the conventional chain for the active
    /// profile is applied automatically (see [`with_defaults`](Self::with_defaults)).
    /// This is what makes `Config::builder(Profile::detect()).build().await`
    /// "just work" — files+env for local/dev/test, Vault+env for staging/prod.
    pub async fn build(self) -> Result<Config, ConfigError> {
        self.ensure_defaults()?.resolve().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::provider::MemoryProvider;
    use serde_json::json;

    #[tokio::test]
    async fn later_providers_override_earlier_ones() {
        let config = ConfigBuilder::new(Profile::Test)
            .with_provider(MemoryProvider::new().set("port", 8080).set("debug", false))
            .with_provider(MemoryProvider::new().set("debug", true))
            .build()
            .await
            .unwrap();

        assert_eq!(config.get_raw("port"), Some(&json!(8080)));
        assert_eq!(config.get_raw("debug"), Some(&json!(true)));
    }

    #[tokio::test]
    async fn vault_profile_without_vault_provider_is_rejected() {
        let err = ConfigBuilder::new(Profile::Prod)
            .with_provider(MemoryProvider::new().set("x", 1))
            .build()
            .await
            .unwrap_err();

        assert!(matches!(err, ConfigError::InvalidProviderChain { .. }));
    }

    #[tokio::test]
    async fn vault_profile_rejects_file_provider() {
        use crate::config::provider::FileProvider;

        let err = ConfigBuilder::new(Profile::Staging)
            .with_provider(FileProvider::optional("config/staging.toml"))
            .build()
            .await
            .unwrap_err();

        assert!(matches!(err, ConfigError::InvalidProviderChain { .. }));
    }

    #[tokio::test]
    async fn empty_builder_auto_applies_defaults_for_file_profile() {
        // No providers registered + a file-secret profile → build() auto-applies
        // with_defaults() (optional files + env) and succeeds even when no config
        // files are present.
        let config = ConfigBuilder::new(Profile::Test).build().await.unwrap();
        assert_eq!(*config.profile(), Profile::Test);
    }

    #[tokio::test]
    async fn file_secret_profile_allows_memory_only_chain() {
        let config = ConfigBuilder::new(Profile::Local)
            .with_provider(MemoryProvider::new().set("ok", true))
            .build()
            .await
            .unwrap();
        assert_eq!(config.get_raw("ok"), Some(&json!(true)));
    }
}
