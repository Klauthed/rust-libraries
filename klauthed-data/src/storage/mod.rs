//! Object storage connections from a [`StorageConfig`](klauthed_core::config::StorageConfig).
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
//! [`DataError::FeatureDisabled`](crate::error::DataError::FeatureDisabled) rather than silently falling back.

mod connect;

pub use connect::*;
