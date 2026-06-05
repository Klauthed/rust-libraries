//! End-to-end config example.
//!
//! Run it with:
//!
//! ```sh
//! cargo run -p klauthed-core --example config_quickstart
//! # override anything via env (highest precedence):
//! APP_SERVER__PORT=9999 cargo run -p klauthed-core --example config_quickstart
//! ```
//!
//! It assembles a provider chain (in-memory defaults → TOML file → environment),
//! resolves it, and reads the pre-built typed sections back out.

use klauthed_core::config::provider::{EnvProvider, FileProvider, MemoryProvider};
use klauthed_core::config::{Config, Profile};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Robustly locate the bundled sample file regardless of the current dir.
    let app_toml = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/config/app.toml");

    let config = Config::builder(Profile::detect())
        // 1. built-in defaults (lowest precedence)
        .with_provider(MemoryProvider::new().set("server", json!({ "port": 8080 })))
        // 2. the service's config file — overrides the defaults above
        .with_provider(FileProvider::new(app_toml))
        // 3. environment overrides, e.g. APP_SERVER__PORT=9999 (highest precedence)
        .with_provider(EnvProvider::new())
        .build()
        .await?;

    println!("=== CONFIG QUICKSTART ===");

    println!("profile          : {}", config.profile());

    let server = config.server()?;
    // 8088 from the file overrides the 8080 default — unless APP_SERVER__PORT is set.
    println!("server bind      : {}", server.bind_address());

    let db = config.database()?;
    println!("database system  : {:?}", db.system);
    println!("database url     : {}", db.connection_url());
    println!("database pool max: {}", db.pool.max_connections);
    println!("database options : {:?}", db.options);

    let cache = config.cache()?;
    println!("cache url        : {:?}", cache.connection_url());

    let messaging = config.messaging()?;
    println!("messaging backend: {:?}", messaging.backend());

    let storage = config.storage()?;
    println!("storage          : {storage:?}");

    Ok(())
}
