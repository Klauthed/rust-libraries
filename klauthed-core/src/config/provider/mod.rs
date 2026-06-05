//! Configuration providers: pluggable sources that each contribute a slice of
//! the resolved config tree.
//!
//! A provider returns a [`ConfigMap`] of top-level keys to (possibly nested)
//! values. The [`ConfigBuilder`](crate::config::ConfigBuilder) loads providers
//! in order and deep-merges them, so a provider later in the chain overrides
//! earlier ones key-by-key.
//!
//! Each provider also declares its [`ProviderKind`], which the builder uses to
//! enforce profile policy — e.g. file secrets are rejected in profiles that
//! require Vault.

use crate::config::map::ConfigMap;

pub mod env;
pub mod file;
pub mod memory;
#[cfg(feature = "vault")]
pub mod vault;

pub use env::EnvProvider;
pub use file::FileProvider;
pub use memory::MemoryProvider;
#[cfg(feature = "vault")]
pub use vault::{VaultAuth, VaultProvider};

use crate::error::ConfigError;

/// Classifies a provider so policy (e.g. "staging/prod must use Vault") can be
/// enforced over a heterogeneous provider chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    /// Process environment variables.
    Env,
    /// On-disk config file (TOML/JSON).
    File,
    /// HashiCorp Vault.
    Vault,
    /// In-memory values supplied programmatically (defaults, tests, overrides).
    Memory,
}

impl ProviderKind {
    /// Whether this kind sources secrets from files/env, which is disallowed in
    /// Vault-only profiles.
    pub fn is_file_secret_source(&self) -> bool {
        matches!(self, ProviderKind::File)
    }
}

/// A pluggable configuration source.
///
/// Implementations are async because some sources (Vault) perform network I/O.
/// Loading happens once at startup; reads afterwards are synchronous off the
/// resolved [`Config`](crate::config::Config).
#[async_trait::async_trait]
pub trait ConfigProvider: Send + Sync {
    /// A short, human-readable name used in diagnostics (e.g. `"env"`, `"file:config/local.toml"`).
    fn name(&self) -> String;

    /// The category of this provider, used for profile policy enforcement.
    fn kind(&self) -> ProviderKind;

    /// Load this provider's contribution to the config tree.
    async fn load(&self) -> Result<ConfigMap, ConfigError>;
}
