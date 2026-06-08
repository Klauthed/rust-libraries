//! [`AppContext`] — a small, explicit application wiring container.
//!
//! Rust has no runtime reflection, so this is **not** an autowiring DI container
//! like Spring's. It is a type-keyed registry of shared singletons: you
//! construct components in dependency order (wiring them through their own
//! constructors), register each, then resolve them by type — the same shape as
//! actix's `app_data` or axum's `Extension`, but framework-agnostic and paired
//! with [`FromConfig`](crate::config::FromConfig) so config-bound components wire
//! in one step.
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
}
