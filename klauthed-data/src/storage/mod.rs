//! Object storage connections from a [`StorageConfig`].
//!
//! Returns a unified [`object_store::ObjectStore`] handle regardless of backend,
//! so call sites are storage-agnostic. The local filesystem backend is always
//! available; the cloud backends are each behind a feature:
//!
//! * `storage-s3`    → Amazon S3 / S3-compatible (MinIO, …)
//! * `storage-gcs`   → Google Cloud Storage
//! * `storage-azure` → Azure Blob Storage
//!
//! Selecting a cloud backend in config without its feature enabled returns
//! [`DataError::FeatureDisabled`] rather than silently falling back.

use std::sync::Arc;

use klauthed_core::config::StorageConfig;
use object_store::ObjectStore;

use crate::error::DataError;

/// Build an [`ObjectStore`] for the configured backend.
pub async fn connect(config: &StorageConfig) -> Result<Arc<dyn ObjectStore>, DataError> {
    match config {
        StorageConfig::Local { root } => {
            // Create the root eagerly so a fresh deployment "just works".
            std::fs::create_dir_all(root)?;
            let store = object_store::local::LocalFileSystem::new_with_prefix(root)?;
            Ok(Arc::new(store))
        }

        StorageConfig::S3 {
            bucket,
            region,
            endpoint,
            access_key_id,
            secret_access_key,
            path_style,
        } => {
            #[cfg(feature = "storage-s3")]
            {
                let mut builder = object_store::aws::AmazonS3Builder::new()
                    .with_bucket_name(bucket.clone())
                    .with_region(region.clone())
                    // path_style = true means *not* virtual-hosted addressing.
                    .with_virtual_hosted_style_request(!path_style);
                if let Some(endpoint) = endpoint {
                    builder = builder.with_endpoint(endpoint.clone());
                    if endpoint.starts_with("http://") {
                        builder = builder.with_allow_http(true);
                    }
                }
                if let Some(key) = access_key_id {
                    builder = builder.with_access_key_id(key.clone());
                }
                if let Some(secret) = secret_access_key {
                    builder = builder.with_secret_access_key(secret.clone());
                }
                let store: Arc<dyn ObjectStore> = Arc::new(builder.build()?);
                Ok(store)
            }
            #[cfg(not(feature = "storage-s3"))]
            {
                let _ = (bucket, region, endpoint, access_key_id, secret_access_key, path_style);
                Err(DataError::FeatureDisabled("storage-s3"))
            }
        }

        StorageConfig::Gcs { bucket, credentials_path } => {
            #[cfg(feature = "storage-gcs")]
            {
                let mut builder = object_store::gcp::GoogleCloudStorageBuilder::new()
                    .with_bucket_name(bucket.clone());
                if let Some(path) = credentials_path {
                    builder =
                        builder.with_service_account_path(path.to_string_lossy().into_owned());
                }
                let store: Arc<dyn ObjectStore> = Arc::new(builder.build()?);
                Ok(store)
            }
            #[cfg(not(feature = "storage-gcs"))]
            {
                let _ = (bucket, credentials_path);
                Err(DataError::FeatureDisabled("storage-gcs"))
            }
        }

        StorageConfig::Azure { account, container, access_key } => {
            #[cfg(feature = "storage-azure")]
            {
                let mut builder = object_store::azure::MicrosoftAzureBuilder::new()
                    .with_account(account.clone())
                    .with_container_name(container.clone());
                if let Some(key) = access_key {
                    builder = builder.with_access_key(key.clone());
                }
                let store: Arc<dyn ObjectStore> = Arc::new(builder.build()?);
                Ok(store)
            }
            #[cfg(not(feature = "storage-azure"))]
            {
                let _ = (account, container, access_key);
                Err(DataError::FeatureDisabled("storage-azure"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use object_store::ObjectStoreExt;
    use object_store::path::Path as ObjectPath;

    #[tokio::test]
    async fn local_backend_round_trips() {
        let dir = std::env::temp_dir().join(format!("klauthed-store-{}", std::process::id()));
        let config = StorageConfig::Local { root: dir.clone() };

        let store = connect(&config).await.expect("local store connects");

        let path = ObjectPath::from("greeting.txt");
        store.put(&path, "hello".into()).await.expect("put");
        let bytes = store.get(&path).await.expect("get").bytes().await.expect("bytes");
        assert_eq!(&bytes[..], b"hello");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[cfg(not(feature = "storage-s3"))]
    #[tokio::test]
    async fn s3_without_feature_reports_disabled() {
        let config = StorageConfig::S3 {
            bucket: "b".into(),
            region: "us-east-1".into(),
            endpoint: None,
            access_key_id: None,
            secret_access_key: None,
            path_style: false,
        };
        let err = connect(&config).await.unwrap_err();
        assert!(matches!(err, DataError::FeatureDisabled("storage-s3")));
    }
}
