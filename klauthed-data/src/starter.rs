//! [`DataStarter`] — wire data-layer resources from config into an `AppContext`
//! (`feature = "sql"`).

use async_trait::async_trait;
use klauthed_core::config::{Config, DatabaseConfig};
use klauthed_core::wiring::{AppContext, Starter, StarterError};

/// A [`Starter`] that builds the relational
/// connection pool from the `database` config section and registers it
/// ([`sqlx::AnyPool`]) in the [`AppContext`] — so components resolve it with
/// `ctx.require::<sqlx::AnyPool>()` instead of connecting by hand.
///
/// A missing `database` section is a no-op. The configured `system` must be
/// relational and its driver feature (`postgres` / `mysql` / `sqlite`) enabled.
/// (Cache / Mongo / messaging wiring is planned; this currently wires the SQL
/// pool.)
///
/// ```no_run
/// use klauthed_core::wiring::AppBuilder;
/// use klauthed_data::DataStarter;
///
/// # async fn run(config: klauthed_core::config::Config)
/// #     -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
/// let ctx = AppBuilder::new(config).with_starter(DataStarter).build().await?;
/// // Components can now resolve the pool: `ctx.require::<sqlx::AnyPool>()`.
/// # let _ = ctx;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Default, Clone)]
pub struct DataStarter;

#[async_trait]
impl Starter for DataStarter {
    fn name(&self) -> &str {
        "data"
    }

    async fn configure(&self, config: &Config, ctx: &mut AppContext) -> Result<(), StarterError> {
        if let Some(database) = config.get_optional::<DatabaseConfig>("database")? {
            let pool = crate::db::connect(&database).await?;
            ctx.register(pool);
        }
        Ok(())
    }
}

#[cfg(all(test, feature = "sqlite"))]
mod tests {
    use super::*;
    use klauthed_core::config::provider::MemoryProvider;
    use klauthed_core::config::{ConfigBuilder, Profile};
    use klauthed_core::wiring::AppBuilder;
    use serde_json::json;

    #[tokio::test]
    async fn registers_an_anypool_from_the_database_section() {
        let config = ConfigBuilder::new(Profile::Test)
            .with_provider(
                MemoryProvider::new()
                    .set("database", json!({ "system": "sqlite", "url": "sqlite::memory:" })),
            )
            .build()
            .await
            .unwrap();

        let ctx = AppBuilder::new(config).with_starter(DataStarter).build().await.unwrap();

        let pool = ctx.require::<sqlx::AnyPool>().unwrap();
        assert!(!pool.is_closed());
    }

    #[tokio::test]
    async fn no_database_section_is_a_noop() {
        let config = ConfigBuilder::new(Profile::Test)
            .with_provider(MemoryProvider::new().set("unrelated", json!(true)))
            .build()
            .await
            .unwrap();

        let ctx = AppBuilder::new(config).with_starter(DataStarter).build().await.unwrap();
        assert!(ctx.get::<sqlx::AnyPool>().is_none());
    }
}
