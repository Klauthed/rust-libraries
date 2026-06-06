//! MongoDB-backed [`IdempotencyStore`].
//!
//! Documents in the collection follow this shape:
//!
//! ```text
//! {
//!   _id:        <idempotency-key>,
//!   status:     "in_progress" | "completed",
//!   response:   <BSON value> | null,
//!   created_at: <RFC3339 string>,
//!   updated_at: <RFC3339 string>,
//!   expires_at: <RFC3339 string>,
//! }
//! ```
//!
//! **begin** — `find_one_and_update` with `$setOnInsert` and `upsert: true`,
//! `returnDocument: Before`. A `None` result means the document was newly
//! inserted (`Outcome::New`); a returned doc reveals the existing status.
//!
//! **complete** — `update_one({_id: key}, {$set: {status, response, updated_at}})`.
//!
//! **get** — `find_one({_id: key})`.
//!
//! A TTL index on `expires_at` auto-expires keys; default TTL is 24 hours,
//! configurable with [`MongoIdempotencyStore::with_ttl_secs`].
//!
//! Live tests are marked `#[ignore]`; run with a running MongoDB at
//! `MONGODB_URL` via:
//! ```text
//! cargo test -p klauthed-data --features mongodb -- --ignored
//! ```

use async_trait::async_trait;
use klauthed_core::time::Timestamp;
use mongodb::Collection;
use mongodb::bson::{Bson, Document, doc, from_bson, to_bson};
use mongodb::options::{FindOneAndUpdateOptions, IndexOptions, ReturnDocument};
use mongodb::{Database, IndexModel};

use crate::error::DataError;
use crate::idempotency::{IdempotencyRecord, IdempotencyStatus, IdempotencyStore, Outcome};

/// Default TTL for idempotency keys: 24 hours in seconds.
const DEFAULT_TTL_SECS: u64 = 24 * 60 * 60;

/// Default collection name for idempotency documents.
const DEFAULT_COLLECTION: &str = "idempotency_keys";

/// A MongoDB-backed [`IdempotencyStore`].
///
/// Clone-cheap: holds only the collection handle (an `Arc` internally).
#[derive(Clone)]
pub struct MongoIdempotencyStore {
    collection: Collection<Document>,
    ttl_secs: u64,
}

impl MongoIdempotencyStore {
    /// Wrap an existing database with the default 24-hour TTL.
    pub fn new(db: &Database) -> Self {
        Self::with_collection(db, DEFAULT_COLLECTION)
    }

    /// Wrap an existing database using `collection_name`.
    pub fn with_collection(db: &Database, collection_name: &str) -> Self {
        Self {
            collection: db.collection(collection_name),
            ttl_secs: DEFAULT_TTL_SECS,
        }
    }

    /// Set a custom TTL in seconds for idempotency keys.
    pub fn with_ttl_secs(mut self, ttl_secs: u64) -> Self {
        self.ttl_secs = ttl_secs;
        self
    }

    /// Create the TTL index on `expires_at`.
    ///
    /// Must be called once (or at each startup — it is idempotent).
    pub async fn ensure_schema(&self) -> Result<(), DataError> {
        let index = IndexModel::builder()
            .keys(doc! { "expires_at": 1 })
            .options(
                IndexOptions::builder()
                    .name(Some("expires_at_ttl".to_owned()))
                    .expire_after(Some(std::time::Duration::from_secs(0)))
                    .build(),
            )
            .build();

        self.collection
            .create_index(index)
            .await
            .map_err(|e| DataError::Idempotency(format!("mongo create TTL index failed: {e}")))?;

        Ok(())
    }

    /// Compute the `expires_at` timestamp for a newly claimed key.
    fn expires_at(&self) -> Result<Timestamp, DataError> {
        let now = Timestamp::now();
        let ttl = klauthed_core::time::Duration::seconds(self.ttl_secs as i64);
        now.checked_add(ttl)
            .ok_or_else(|| DataError::Idempotency("TTL overflow".to_owned()))
    }
}

fn status_to_str(status: IdempotencyStatus) -> &'static str {
    match status {
        IdempotencyStatus::InProgress => "in_progress",
        IdempotencyStatus::Completed => "completed",
    }
}

fn str_to_status(s: &str) -> Result<IdempotencyStatus, DataError> {
    match s {
        "in_progress" => Ok(IdempotencyStatus::InProgress),
        "completed" => Ok(IdempotencyStatus::Completed),
        other => Err(DataError::Idempotency(format!(
            "unknown idempotency status '{other}'"
        ))),
    }
}

fn parse_timestamp(s: &str) -> Result<Timestamp, DataError> {
    serde_json::from_value(serde_json::Value::String(s.to_owned()))
        .map_err(|e| DataError::Idempotency(format!("invalid timestamp '{s}': {e}")))
}

fn doc_to_record(key: &str, doc: &Document) -> Result<IdempotencyRecord, DataError> {
    let status_str = doc
        .get_str("status")
        .map_err(|e| DataError::Idempotency(format!("missing status: {e}")))?;
    let status = str_to_status(status_str)?;

    let response: Option<serde_json::Value> = match doc.get("response") {
        Some(Bson::Null) | None => None,
        Some(bson) => Some(
            from_bson(bson.clone())
                .map_err(|e| DataError::Idempotency(format!("response bson→json: {e}")))?,
        ),
    };

    let created_at = parse_timestamp(
        doc.get_str("created_at")
            .map_err(|e| DataError::Idempotency(format!("missing created_at: {e}")))?,
    )?;
    let updated_at = parse_timestamp(
        doc.get_str("updated_at")
            .map_err(|e| DataError::Idempotency(format!("missing updated_at: {e}")))?,
    )?;

    Ok(IdempotencyRecord {
        key: key.to_owned(),
        status,
        response,
        created_at,
        updated_at,
    })
}

#[async_trait]
impl IdempotencyStore for MongoIdempotencyStore {
    async fn begin(&self, key: &str) -> Result<Outcome, DataError> {
        let now = Timestamp::now();
        let expires_at = self.expires_at()?;
        let now_str = now.to_rfc3339();
        let expires_str = expires_at.to_rfc3339();

        // `$setOnInsert` only writes when the document is being inserted (new key).
        // `returnDocument: Before` → None means the doc was just created.
        let filter = doc! { "_id": key };
        let update = doc! {
            "$setOnInsert": {
                "status":     status_to_str(IdempotencyStatus::InProgress),
                "response":   Bson::Null,
                "created_at": &now_str,
                "updated_at": &now_str,
                "expires_at": &expires_str,
            }
        };
        let options = FindOneAndUpdateOptions::builder()
            .upsert(Some(true))
            .return_document(Some(ReturnDocument::Before))
            .build();

        let existing = self
            .collection
            .find_one_and_update(filter, update)
            .with_options(options)
            .await
            .map_err(|e| DataError::Idempotency(format!("mongo find_one_and_update failed: {e}")))?;

        match existing {
            // Document did not exist before — we just inserted it.
            None => Ok(Outcome::New),
            // Document already existed; inspect its status.
            Some(doc) => {
                let record = doc_to_record(key, &doc)?;
                match record.status {
                    IdempotencyStatus::InProgress => Ok(Outcome::InProgress),
                    IdempotencyStatus::Completed => {
                        let response = record.response.unwrap_or(serde_json::Value::Null);
                        Ok(Outcome::Completed(response))
                    }
                }
            }
        }
    }

    async fn complete(&self, key: &str, response: serde_json::Value) -> Result<(), DataError> {
        let now = Timestamp::now().to_rfc3339();
        let response_bson = to_bson(&response)
            .map_err(|e| DataError::Idempotency(format!("json→bson failed: {e}")))?;

        let filter = doc! { "_id": key };
        let update = doc! {
            "$set": {
                "status":     status_to_str(IdempotencyStatus::Completed),
                "response":   response_bson,
                "updated_at": now,
            }
        };

        let result = self
            .collection
            .update_one(filter, update)
            .await
            .map_err(|e| DataError::Idempotency(format!("mongo update_one failed: {e}")))?;

        if result.matched_count == 0 {
            return Err(DataError::Idempotency(format!(
                "cannot complete unknown idempotency key '{key}'"
            )));
        }

        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<IdempotencyRecord>, DataError> {
        let filter = doc! { "_id": key };
        let doc = self
            .collection
            .find_one(filter)
            .await
            .map_err(|e| DataError::Idempotency(format!("mongo find_one failed: {e}")))?;

        match doc {
            None => Ok(None),
            Some(d) => Ok(Some(doc_to_record(key, &d)?)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use klauthed_core::id::Id;

    async fn live_store() -> MongoIdempotencyStore {
        let url = std::env::var("MONGODB_URL")
            .unwrap_or_else(|_| "mongodb://127.0.0.1:27017".to_owned());
        let client = mongodb::Client::with_uri_str(&url)
            .await
            .expect("connect mongodb");
        let db_name = format!("klauthed_test_{}", Id::<()>::new());
        let store = MongoIdempotencyStore::new(&client.database(&db_name));
        store.ensure_schema().await.expect("ensure schema");
        store
    }

    #[tokio::test]
    #[ignore = "requires a live MongoDB at MONGODB_URL"]
    async fn new_in_progress_complete_replay() {
        let store = live_store().await;
        let key = format!("test:{}", Id::<()>::new());

        assert_eq!(store.begin(&key).await.unwrap(), Outcome::New);
        assert_eq!(store.begin(&key).await.unwrap(), Outcome::InProgress);

        let response = serde_json::json!({ "charged": true });
        store.complete(&key, response.clone()).await.unwrap();

        assert_eq!(
            store.begin(&key).await.unwrap(),
            Outcome::Completed(response)
        );
    }

    #[tokio::test]
    #[ignore = "requires a live MongoDB at MONGODB_URL"]
    async fn complete_unknown_key_errors() {
        let store = live_store().await;
        let key = format!("test:{}:missing", Id::<()>::new());
        let err = store
            .complete(&key, serde_json::Value::Null)
            .await
            .unwrap_err();
        assert!(matches!(err, DataError::Idempotency(_)));
    }
}
