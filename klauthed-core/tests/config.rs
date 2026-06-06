//! Integration tests for the config system.
//!
//! Unlike the in-file `#[cfg(test)] mod tests` unit tests (which can reach
//! private items), these exercise only the public API — exactly what a
//! downstream service crate would see.

use klauthed_core::config::provider::{FileProvider, MemoryProvider};
use klauthed_core::config::{Config, MessagingBackend, Profile};
use serde_json::json;

#[tokio::test]
async fn builds_and_reads_typed_sections() {
    let config = Config::builder(Profile::Test)
        .with_provider(
            MemoryProvider::new()
                .set("server", json!({ "host": "127.0.0.1", "port": 9000 }))
                .set(
                    "database",
                    json!({
                        "system": "mysql",
                        "host": "db",
                        "database": "app",
                        "username": "u",
                        "password": "p"
                    }),
                )
                .set("messaging", json!({ "backend": "kafka", "brokers": ["k:9092"] })),
        )
        .build()
        .await
        .expect("config builds");

    let server = config.server().expect("server section");
    assert_eq!(server.bind_address(), "127.0.0.1:9000");

    let db = config.database().expect("database section");
    assert_eq!(db.effective_port(), Some(3306));
    assert_eq!(db.connection_url(), "mysql://u:p@db:3306/app");

    let messaging = config.messaging().expect("messaging section");
    assert_eq!(messaging.backend(), MessagingBackend::Kafka);
}

#[tokio::test]
async fn later_providers_override_earlier_ones_per_key() {
    let config = Config::builder(Profile::Local)
        .with_provider(
            MemoryProvider::new().set("server", json!({ "host": "0.0.0.0", "port": 8080 })),
        )
        .with_provider(MemoryProvider::new().set("server", json!({ "port": 8088 })))
        .build()
        .await
        .unwrap();

    let server = config.server().unwrap();
    // host survives from the first layer; port is overridden by the second.
    assert_eq!(server.bind_address(), "0.0.0.0:8088");
}

#[tokio::test]
async fn loads_and_resolves_a_toml_file() {
    let dir = std::env::temp_dir().join(format!("klauthed-cfg-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("app.toml");
    std::fs::write(
        &path,
        r#"
        [database]
        system   = "postgres"
        database = "app"
        "#,
    )
    .unwrap();

    let config = Config::builder(Profile::Local)
        .with_provider(FileProvider::new(&path))
        .build()
        .await
        .unwrap();

    assert_eq!(config.database().unwrap().connection_url(), "postgres://localhost:5432/app");

    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn prod_profile_requires_vault() {
    let err = Config::builder(Profile::Prod)
        .with_provider(MemoryProvider::new().set("x", json!(1)))
        .build()
        .await
        .expect_err("prod must reject a chain with no Vault provider");

    assert!(err.to_string().to_lowercase().contains("vault"));
}
