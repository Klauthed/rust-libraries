//! `klauthed-data::migrate`: the embedded `Migrator` applies versioned SQL
//! migrations in order — each in its own transaction — and records them in a
//! `_klauthed_migrations` table so re-runs are idempotent. Shown here against an
//! in-memory SQLite database opened through the normal `db::connect` path.

use klauthed_core::config::{DatabaseConfig, DbSystem, PoolConfig};
use klauthed_data::db;
use klauthed_data::migrate::{Migration, Migrator};

pub async fn run() {
    // An in-memory SQLite database. `sqlite::memory:` is private per connection,
    // so pin the pool to one connection — otherwise queries could land on
    // different, empty databases.
    let config = DatabaseConfig {
        system: DbSystem::Sqlite,
        url: Some("sqlite::memory:".to_owned()),
        pool: PoolConfig { max_connections: 1, ..Default::default() },
        ..Default::default()
    };
    let pool = db::connect(&config).await.unwrap();

    let migrator = Migrator::new([
        Migration::new(1, "create_users", "CREATE TABLE users (id BIGINT PRIMARY KEY)"),
        Migration::new(2, "add_email", "ALTER TABLE users ADD COLUMN email TEXT"),
    ])
    .unwrap();

    // First run applies both pending migrations.
    let applied = migrator.run(&pool).await.unwrap();
    let versions = migrator.applied(&pool).await.unwrap();
    println!("  first run applied {applied} migration(s); recorded versions {versions:?}");
    assert_eq!(applied, 2);
    assert_eq!(versions, vec![1, 2]);

    // Re-running applies nothing — already-recorded versions are skipped.
    let again = migrator.run(&pool).await.unwrap();
    println!("  second run applied {again} (idempotent)");
    assert_eq!(again, 0);
}
