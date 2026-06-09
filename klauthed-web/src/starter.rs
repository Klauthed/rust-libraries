//! [`WebStarter`] — assemble the actix [`Components`] from wired resources.

use async_trait::async_trait;
use klauthed_core::config::Config;
use klauthed_core::wiring::{AppContext, Starter, StarterError};

use crate::app::Components;

/// A [`Starter`] that assembles the web
/// [`Components`] from resources already registered in the [`AppContext`] — the
/// `sqlx::AnyPool` left by `DataStarter`, a Redis connection from a cache
/// starter — wiring each as `web::Data` plus its readiness health check, then
/// registers the resulting [`Components`] for
/// [`serve_with_components`](crate::server::serve_with_components) to consume.
///
/// Add it **after** the data/cache starters so their resources are present:
///
/// ```no_run
/// use klauthed_core::wiring::AppBuilder;
/// use klauthed_web::WebStarter;
///
/// # async fn run(config: klauthed_core::config::Config)
/// #     -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
/// let ctx = AppBuilder::new(config)
///     // .with_starter(klauthed_data::DataStarter)
///     .with_starter(WebStarter)
///     .build()
///     .await?;
/// let components = ctx.require::<klauthed_web::Components>()?;
/// # let _ = components;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Default, Clone)]
pub struct WebStarter;

#[async_trait]
impl Starter for WebStarter {
    fn name(&self) -> &str {
        "web"
    }

    async fn configure(&self, _config: &Config, ctx: &mut AppContext) -> Result<(), StarterError> {
        // `mut` is used only when a resource-specific feature is enabled.
        #[allow(unused_mut)]
        let mut components = Components::new();

        #[cfg(feature = "data-sql")]
        if let Some(pool) = ctx.get::<sqlx::AnyPool>() {
            components = components.pool("database", (*pool).clone());
        }

        #[cfg(feature = "data-redis")]
        if let Some(conn) = ctx.get::<redis::aio::ConnectionManager>() {
            components = components.redis("redis", (*conn).clone());
        }

        ctx.register(components);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use klauthed_core::config::provider::MemoryProvider;
    use klauthed_core::config::{ConfigBuilder, Profile};
    use klauthed_core::wiring::AppBuilder;
    use serde_json::json;

    async fn test_config() -> Config {
        ConfigBuilder::new(Profile::Test)
            .with_provider(MemoryProvider::new().set("server", json!({ "port": 8080 })))
            .build()
            .await
            .unwrap()
    }

    #[actix_web::test]
    async fn registers_empty_components_with_no_resources() {
        let ctx =
            AppBuilder::new(test_config().await).with_starter(WebStarter).build().await.unwrap();

        let components = ctx.require::<Components>().unwrap();
        assert_eq!(components.check_count(), 0);
    }

    // The pool composition (DataStarter -> AnyPool -> Components SQL check) needs
    // the SQLite driver, enabled via a dev-dependency.
    #[cfg(feature = "data-sql")]
    #[actix_web::test]
    async fn composes_a_health_check_from_a_registered_pool() {
        let config = ConfigBuilder::new(Profile::Test)
            .with_provider(
                MemoryProvider::new()
                    .set("database", json!({ "system": "sqlite", "url": "sqlite::memory:" })),
            )
            .build()
            .await
            .unwrap();

        let ctx = AppBuilder::new(config)
            .with_starter(klauthed_data::DataStarter)
            .with_starter(WebStarter)
            .build()
            .await
            .unwrap();

        // DataStarter registered the pool; WebStarter turned it into a SQL check.
        let components = ctx.require::<Components>().unwrap();
        assert_eq!(components.check_count(), 1);
    }
}
