//! Wire a `StorageConfig` into a real, working object store (local filesystem).
//!
//! Run with:
//!
//! ```sh
//! cargo run -p klauthed-data --example local_storage --features storage
//! ```
//!
//! Uses the local filesystem backend, so it needs no cloud credentials and runs
//! anywhere. The same `storage::connect` call serves S3/GCS/Azure when their
//! features are enabled and the config selects them.

use klauthed_core::config::StorageConfig;
use klauthed_data::storage;
use object_store::ObjectStoreExt;
use object_store::path::Path as ObjectPath;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // In a real service this comes from `Config::load().await?.storage()?`.
    let config = StorageConfig::Local { root: std::env::temp_dir().join("klauthed-example-store") };

    let store = storage::connect(&config).await?;

    let path = ObjectPath::from("notes/hello.txt");
    store.put(&path, "hello from klauthed".into()).await?;

    let bytes = store.get(&path).await?.bytes().await?;
    println!("stored at : {path}");
    println!("read back : {}", String::from_utf8_lossy(&bytes));

    store.delete(&path).await?;
    println!("deleted   : {path}");

    Ok(())
}
