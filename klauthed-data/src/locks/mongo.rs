//! MongoDB-backed [`LockManager`] using compare-and-upsert with TTL.
//!
//! Each lock is a document in a MongoDB collection:
//!
//! ```text
//! { _id: <key>, token: <uuid-string>, expires_at: <ISO8601 string> }
//! ```
//!
//! **Acquire** — `find_one_and_update` with:
//! * filter `{_id: key, expires_at: {$lte: now}}` (matches absent or expired docs),
//! * update `{$set: {token: new_token, expires_at: now+ttl}}`,
//! * `upsert: true`, `return_document: After`.
//!
//! If the document is held by a live token the filter matches nothing and
//! `find_one_and_update` returns `None`, mapped to `Ok(None)` (contention).
//! A duplicate-key error on upsert (race between two writers both seeing the
//! key absent) is also treated as contention.
//!
//! **Release** — `delete_one({_id: key, token: token_string})` — compare-and-
//! delete so only the holder that set this token removes the document.
//!
//! Live tests are marked `#[ignore]`; run with a running MongoDB at
//! `MONGODB_URL` via:
//! ```text
//! cargo test -p klauthed-data --features mongodb -- --ignored
//! ```

use async_trait::async_trait;
use klauthed_core::time::Duration;
use klauthed_core::time::Timestamp;
use mongodb::Collection;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use mongodb::options::{FindOneAndUpdateOptions, ReturnDocument};

use crate::error::DataError;
use crate::locks::{LockGuard, LockManager, LockToken};

/// Default collection name for lock documents.
const DEFAULT_COLLECTION: &str = "locks";

/// A [`LockManager`] that stores TTL-bounded locks in a MongoDB collection.
///
/// Clone-cheap: holds only the collection handle (an `Arc` internally).
#[derive(Clone)]
pub struct MongoLockManager {
    collection: Collection<Document>,
}

impl MongoLockManager {
    /// Wrap an existing database, using the default collection name `"locks"`.
    pub fn new(db: &Database) -> Self {
        Self::with_collection(db, DEFAULT_COLLECTION)
    }

    /// Wrap an existing database, using `collection_name` as the target.
    pub fn with_collection(db: &Database, collection_name: &str) -> Self {
        Self { collection: db.collection(collection_name) }
    }

    /// Release `key` only if `token` still owns it.
    ///
    /// Returns `true` if the lock was held and is now released, `false` if it
    /// had already expired or been taken by a different holder.
    pub async fn release_token(&self, key: &str, token: LockToken) -> Result<bool, DataError> {
        let filter = doc! {
            "_id":   key,
            "token": token.to_string(),
        };
        let result = self
            .collection
            .delete_one(filter)
            .await
            .map_err(|e| DataError::LockHeld(format!("mongo delete_one failed: {e}")))?;
        Ok(result.deleted_count > 0)
    }
}

#[async_trait]
impl LockManager for MongoLockManager {
    async fn acquire(&self, key: &str, ttl: Duration) -> Result<Option<LockGuard>, DataError> {
        let now = Timestamp::now();
        let expires_at = now
            .checked_add(ttl)
            .ok_or_else(|| DataError::LockHeld(format!("invalid TTL for lock '{key}'")))?;

        let now_str = now.to_rfc3339();
        let expires_str = expires_at.to_rfc3339();
        let token = LockToken::new();
        let token_str = token.to_string();

        // Match documents that are absent or whose `expires_at` has passed.
        let filter = doc! {
            "_id":        key,
            "expires_at": { "$lte": &now_str },
        };
        let update = doc! {
            "$set": {
                "token":      &token_str,
                "expires_at": &expires_str,
            }
        };
        let options = FindOneAndUpdateOptions::builder()
            .upsert(Some(true))
            .return_document(Some(ReturnDocument::After))
            .build();

        let result =
            self.collection.find_one_and_update(filter, update).with_options(options).await;

        match result {
            Ok(Some(doc)) => {
                // Verify the returned document has our token — if another writer
                // upserted at the same instant it could carry a different token.
                let doc_token = doc.get_str("token").unwrap_or_default();
                if doc_token == token_str {
                    Ok(Some(LockGuard::mongo(key.to_owned(), token, self.clone())))
                } else {
                    Ok(None)
                }
            }
            // The filter matched nothing (key exists with a live token) — contention.
            Ok(None) => Ok(None),
            Err(e) => {
                // A duplicate-key error means two upserts raced; treat as contention.
                let msg = e.to_string();
                if msg.contains("11000") || msg.contains("DuplicateKey") {
                    Ok(None)
                } else {
                    Err(DataError::LockHeld(format!("mongo find_one_and_update failed: {e}")))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use klauthed_core::id::Id;

    async fn live_manager() -> MongoLockManager {
        let url =
            std::env::var("MONGODB_URL").unwrap_or_else(|_| "mongodb://127.0.0.1:27017".to_owned());
        let client = mongodb::Client::with_uri_str(&url).await.expect("connect mongodb");
        let db_name = format!("klauthed_test_{}", Id::<()>::new());
        MongoLockManager::new(&client.database(&db_name))
    }

    #[tokio::test]
    #[ignore = "requires a live MongoDB at MONGODB_URL"]
    async fn acquire_blocks_and_releases() {
        let locks = live_manager().await;
        let key = format!("klauthed:test:lock:{}", LockToken::new());

        let guard =
            locks.acquire(&key, Duration::seconds(30)).await.unwrap().expect("first acquire wins");

        assert!(locks.acquire(&key, Duration::seconds(30)).await.unwrap().is_none());

        guard.release().await.unwrap();

        assert!(locks.acquire(&key, Duration::seconds(30)).await.unwrap().is_some());
    }
}
