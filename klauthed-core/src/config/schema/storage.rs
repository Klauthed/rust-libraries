use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Object/file storage configuration, tagged on `backend`.
///
/// ```toml
/// [storage]
/// backend = "s3"
/// bucket  = "uploads"
/// region  = "eu-central-1"
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "backend", rename_all = "snake_case")]
pub enum StorageConfig {
    /// Local filesystem storage.
    Local {
        #[serde(default = "default_local_root")]
        root: PathBuf,
    },
    /// Amazon S3 (or S3-compatible endpoints such as MinIO).
    S3 {
        bucket: String,
        #[serde(default = "default_s3_region")]
        region: String,
        /// Custom endpoint for S3-compatible stores (e.g. MinIO).
        #[serde(default)]
        endpoint: Option<String>,
        #[serde(default)]
        access_key_id: Option<String>,
        /// Secret key. Prefer sourcing this from Vault in staging/prod.
        #[serde(default)]
        secret_access_key: Option<String>,
        /// Use path-style addressing (required by some S3-compatible servers).
        #[serde(default)]
        path_style: bool,
    },
    /// Google Cloud Storage.
    Gcs {
        bucket: String,
        /// Path to a service-account credentials JSON file.
        #[serde(default)]
        credentials_path: Option<PathBuf>,
    },
    /// Azure Blob Storage.
    Azure {
        account: String,
        container: String,
        /// Access key. Prefer sourcing this from Vault in staging/prod.
        #[serde(default)]
        access_key: Option<String>,
    },
}

fn default_local_root() -> PathBuf {
    PathBuf::from("./data")
}
fn default_s3_region() -> String {
    "us-east-1".to_owned()
}

impl Default for StorageConfig {
    fn default() -> Self {
        StorageConfig::Local {
            root: default_local_root(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_local_backend_with_default_root() {
        let cfg: StorageConfig = serde_json::from_value(json!({ "backend": "local" })).unwrap();
        assert_eq!(cfg, StorageConfig::Local { root: PathBuf::from("./data") });
    }

    #[test]
    fn parses_s3_backend() {
        let cfg: StorageConfig = serde_json::from_value(json!({
            "backend": "s3",
            "bucket": "uploads",
            "region": "eu-central-1",
            "path_style": true
        }))
        .unwrap();

        assert_eq!(
            cfg,
            StorageConfig::S3 {
                bucket: "uploads".into(),
                region: "eu-central-1".into(),
                endpoint: None,
                access_key_id: None,
                secret_access_key: None,
                path_style: true,
            }
        );
    }
}
