//! [`AppContext`] — a small, explicit application wiring container.
//!
//! Rust has no runtime reflection, so this is **not** an autowiring DI container
//! like Spring's. It is a type-keyed registry of shared singletons: you
//! construct components in dependency order (wiring them through their own
//! constructors), register each, then resolve them by type — the same shape as
//! actix's `app_data` or axum's `Extension`, but framework-agnostic and paired
//! with [`FromConfig`] so config-bound components wire in one step.
//!
//! ```
//! use std::sync::Arc;
//! use klauthed_core::wiring::AppContext;
//!
//! struct Db {
//!     url: String,
//! }
//! struct UserService {
//!     db: Arc<Db>,
//! }
//!
//! let mut ctx = AppContext::new();
//! ctx.register(Db { url: "postgres://…".into() });
//! let db = ctx.require::<Db>().unwrap();
//! ctx.register(UserService { db });
//!
//! let users = ctx.require::<UserService>().unwrap();
//! assert_eq!(users.db.url, "postgres://…");
//! ```

use std::any::{Any, TypeId, type_name};
use std::collections::HashMap;
use std::sync::Arc;

use klauthed_macros::DomainError;

use crate::config::{Config, FromConfig};
use crate::error::ConfigError;

/// Errors raised while resolving components from an [`AppContext`].
#[derive(Debug, DomainError)]
#[domain(prefix = "wiring", category = "internal")]
#[non_exhaustive]
pub enum WiringError {
    /// No component of the requested type was registered.
    #[domain(category = "internal", code = "missing_component")]
    MissingComponent(&'static str),
}

impl std::fmt::Display for WiringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WiringError::MissingComponent(ty) => {
                write!(f, "no component of type `{ty}` is registered")
            }
        }
    }
}

impl std::error::Error for WiringError {}

/// A type-keyed registry of shared application singletons.
///
/// Each type can hold one component (registering the same type again replaces
/// it). Components are stored behind an [`Arc`], so [`get`](Self::get) /
/// [`require`](Self::require) hand out cheap clones that can be shared across
/// tasks. Wrap the whole context in an `Arc` to share it.
#[derive(Default)]
pub struct AppContext {
    components: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
}

impl AppContext {
    /// An empty context.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `component`, replacing any existing one of the same type.
    pub fn register<T: Any + Send + Sync>(&mut self, component: T) -> &mut Self {
        self.register_arc(Arc::new(component))
    }

    /// Register an already-shared `component`.
    pub fn register_arc<T: Any + Send + Sync>(&mut self, component: Arc<T>) -> &mut Self {
        self.components.insert(TypeId::of::<T>(), component);
        self
    }

    /// Register `component` (consuming-builder form, for fluent setup).
    #[must_use]
    pub fn with<T: Any + Send + Sync>(mut self, component: T) -> Self {
        self.register(component);
        self
    }

    /// Bind `T` from `config` via [`FromConfig`] and register it.
    ///
    /// # Errors
    /// Returns [`ConfigError`] if the component cannot be bound from config.
    pub fn register_from_config<T>(&mut self, config: &Config) -> Result<&mut Self, ConfigError>
    where
        T: FromConfig + Any + Send + Sync,
    {
        let component = T::from_config(config)?;
        Ok(self.register(component))
    }

    /// Resolve the component of type `T`, if registered.
    #[must_use]
    pub fn get<T: Any + Send + Sync>(&self) -> Option<Arc<T>> {
        self.components.get(&TypeId::of::<T>()).and_then(|c| Arc::clone(c).downcast::<T>().ok())
    }

    /// Resolve the component of type `T`, or error if it is not registered.
    ///
    /// # Errors
    /// Returns [`WiringError::MissingComponent`] if no `T` was registered.
    pub fn require<T: Any + Send + Sync>(&self) -> Result<Arc<T>, WiringError> {
        self.get::<T>().ok_or(WiringError::MissingComponent(type_name::<T>()))
    }

    /// Whether a component of type `T` is registered.
    #[must_use]
    pub fn contains<T: Any + Send + Sync>(&self) -> bool {
        self.components.contains_key(&TypeId::of::<T>())
    }

    /// The number of registered components.
    #[must_use]
    pub fn len(&self) -> usize {
        self.components.len()
    }

    /// Whether no components are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.components.is_empty()
    }
}

/// The error a [`Starter`] may fail with — boxed so a starter can surface a
/// config error, a connection failure, or anything else that should abort
/// startup (every klauthed error type coerces into it via `?`).
pub type StarterError = Box<dyn std::error::Error + Send + Sync>;

/// A unit of auto-configuration (a "starter"): given the resolved [`Config`], it
/// constructs and registers its components into an [`AppContext`].
///
/// This is the klauthed analog of a Spring Boot starter — each module contributes
/// one and [`AppBuilder`] runs them in order. Rust has no classpath scanning, so
/// starters are composed explicitly rather than discovered. `configure` is async
/// because real starters build live resources (database pools, clients).
#[async_trait::async_trait]
pub trait Starter: Send + Sync {
    /// A short name, for diagnostics.
    fn name(&self) -> &str;

    /// Register this starter's components, reading what it needs from `config`
    /// and building any live resources it provides.
    ///
    /// # Errors
    /// Returns a [`StarterError`] if configuration is missing/malformed or a
    /// resource can't be built — anything that should stop startup.
    async fn configure(&self, config: &Config, ctx: &mut AppContext) -> Result<(), StarterError>;
}

/// Bootstraps an [`AppContext`] by running a chain of [`Starter`]s over a
/// resolved [`Config`].
///
/// The [`Config`] is registered first, so starters and components can resolve it
/// with `ctx.require::<Config>()`.
///
/// ```
/// use klauthed_core::config::ServerConfig;
/// use klauthed_core::config::provider::MemoryProvider;
/// use klauthed_core::config::{ConfigBuilder, Profile};
/// use klauthed_core::wiring::{AppBuilder, ConfigSectionsStarter};
/// use serde_json::json;
///
/// # async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
/// let config = ConfigBuilder::new(Profile::Test)
///     .with_provider(MemoryProvider::new().set("server", json!({ "port": 9000 })))
///     .build()
///     .await?;
///
/// let ctx = AppBuilder::new(config).with_starter(ConfigSectionsStarter).build().await?;
/// assert_eq!(ctx.require::<ServerConfig>()?.port, 9000);
/// # Ok(())
/// # }
/// ```
pub struct AppBuilder {
    config: Config,
    starters: Vec<Box<dyn Starter>>,
}

impl AppBuilder {
    /// Start a builder over the resolved `config`.
    #[must_use]
    pub fn new(config: Config) -> Self {
        Self { config, starters: Vec::new() }
    }

    /// Append a starter to the chain (run in insertion order).
    #[must_use]
    pub fn with_starter<S: Starter + 'static>(mut self, starter: S) -> Self {
        self.starters.push(Box::new(starter));
        self
    }

    /// Append a boxed starter (when the concrete type is only known at runtime).
    #[must_use]
    pub fn with_boxed_starter(mut self, starter: Box<dyn Starter>) -> Self {
        self.starters.push(starter);
        self
    }

    /// Run every starter in order and return the wired [`AppContext`].
    ///
    /// # Errors
    /// Returns the [`StarterError`] from the first starter that fails.
    pub async fn build(self) -> Result<AppContext, StarterError> {
        let mut ctx = AppContext::new();
        ctx.register(self.config.clone());
        for starter in &self.starters {
            tracing::debug!(starter = starter.name(), "running starter");
            starter.configure(&self.config, &mut ctx).await?;
        }
        Ok(ctx)
    }
}

/// A [`Starter`] that registers each present standard typed config section
/// (`DatabaseConfig`, `CacheConfig`, `MessagingConfig`, `StorageConfig`,
/// `ServerConfig`) into the context, so components can `require` them directly
/// instead of re-reading the config.
pub struct ConfigSectionsStarter;

#[async_trait::async_trait]
impl Starter for ConfigSectionsStarter {
    fn name(&self) -> &str {
        "config-sections"
    }

    async fn configure(&self, config: &Config, ctx: &mut AppContext) -> Result<(), StarterError> {
        use crate::config::keys;
        use crate::config::{
            CacheConfig, DatabaseConfig, MessagingConfig, ServerConfig, StorageConfig,
        };

        if let Some(section) = config.get_optional::<DatabaseConfig>(keys::DATABASE)? {
            ctx.register(section);
        }
        if let Some(section) = config.get_optional::<CacheConfig>(keys::CACHE)? {
            ctx.register(section);
        }
        if let Some(section) = config.get_optional::<MessagingConfig>(keys::MESSAGING)? {
            ctx.register(section);
        }
        if let Some(section) = config.get_optional::<StorageConfig>(keys::STORAGE)? {
            ctx.register(section);
        }
        if let Some(section) = config.get_optional::<ServerConfig>(keys::SERVER)? {
            ctx.register(section);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::provider::MemoryProvider;
    use crate::config::{ConfigBuilder, Profile};
    use serde::Deserialize;
    use serde_json::json;

    struct Db {
        url: String,
    }
    struct Cache {
        entries: u32,
    }

    #[test]
    fn register_and_resolve_by_type() {
        let mut ctx = AppContext::new();
        ctx.register(Db { url: "u".into() }).register(Cache { entries: 10 });

        assert_eq!(ctx.len(), 2);
        assert_eq!(ctx.require::<Db>().unwrap().url, "u");
        assert_eq!(ctx.get::<Cache>().unwrap().entries, 10);
    }

    #[test]
    fn missing_component_errors() {
        let ctx = AppContext::new();
        assert!(!ctx.contains::<Db>());
        // `Db` isn't `Debug`, so match the Result rather than `unwrap_err`.
        assert!(matches!(ctx.require::<Db>(), Err(WiringError::MissingComponent(_))));
    }

    #[test]
    fn registering_same_type_replaces() {
        let mut ctx = AppContext::new();
        ctx.register(Db { url: "first".into() });
        ctx.register(Db { url: "second".into() });
        assert_eq!(ctx.len(), 1);
        assert_eq!(ctx.require::<Db>().unwrap().url, "second");
    }

    #[test]
    fn shared_arcs_are_cheap_clones() {
        let mut ctx = AppContext::new();
        ctx.register(Db { url: "u".into() });
        let a = ctx.require::<Db>().unwrap();
        let b = ctx.require::<Db>().unwrap();
        assert!(Arc::ptr_eq(&a, &b));
    }

    #[derive(Deserialize, FromConfig)]
    #[config(key = "database")]
    struct DatabaseSettings {
        host: String,
    }

    #[tokio::test]
    async fn register_from_config_binds_and_registers() {
        let config = ConfigBuilder::new(Profile::Test)
            .with_provider(MemoryProvider::new().set("database", json!({ "host": "db.internal" })))
            .build()
            .await
            .unwrap();

        let mut ctx = AppContext::new();
        ctx.register_from_config::<DatabaseSettings>(&config).unwrap();

        assert_eq!(ctx.require::<DatabaseSettings>().unwrap().host, "db.internal");
    }

    #[tokio::test]
    async fn app_builder_runs_config_sections_starter() {
        use crate::config::{DatabaseConfig, ServerConfig};

        let config = ConfigBuilder::new(Profile::Test)
            .with_provider(MemoryProvider::new().set("server", json!({ "port": 9000 })))
            .build()
            .await
            .unwrap();

        let ctx =
            AppBuilder::new(config).with_starter(ConfigSectionsStarter).build().await.unwrap();

        // The Config itself and the present `server` section are registered.
        assert!(ctx.contains::<Config>());
        assert_eq!(ctx.require::<ServerConfig>().unwrap().port, 9000);
        // An absent section is not registered.
        assert!(!ctx.contains::<DatabaseConfig>());
    }

    #[tokio::test]
    async fn app_builder_runs_a_custom_starter() {
        struct DbStarter;
        #[async_trait::async_trait]
        impl Starter for DbStarter {
            fn name(&self) -> &str {
                "db"
            }
            async fn configure(
                &self,
                _config: &Config,
                ctx: &mut AppContext,
            ) -> Result<(), StarterError> {
                ctx.register(Db { url: "wired".into() });
                Ok(())
            }
        }

        let config = ConfigBuilder::new(Profile::Test)
            .with_provider(MemoryProvider::new().set("x", json!(1)))
            .build()
            .await
            .unwrap();

        let ctx = AppBuilder::new(config).with_starter(DbStarter).build().await.unwrap();
        assert_eq!(ctx.require::<Db>().unwrap().url, "wired");
    }
}
