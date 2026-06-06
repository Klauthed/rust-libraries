//! MongoDB-backed [`Outbox`].
//!
//! Persists outbox entries as BSON documents in a MongoDB collection.
//! Each document maps one [`OutboxEntry`] field-for-field:
//!
//! | BSON field      | Rust field                    |
//! |-----------------|-------------------------------|
//! | `_id`           | `id` (UUID v7, as String)     |
//! | `aggregate_type`| `aggregate_type`              |
//! | `aggregate_id`  | `aggregate_id`                |
//! | `event_type`    | `event_type`                  |
//! | `sequence`      | `sequence` (i64)              |
//! | `payload`       | `payload` (JSON as BSON)      |
//! | `occurred_at`   | RFC3339 String                |
//! | `published`     | bool                          |
//! | `published_at`  | RFC3339 String or null        |
//!
//! [`ensure_schema`](MongoOutbox::ensure_schema) creates:
//! * a unique index on `{_id: 1}` (the default `_id` index, already present),
//! * a compound index on `{published: 1, sequence: 1}` for relay queries,
//! * a sparse index on `{aggregate_id: 1}` for per-aggregate look-ups.
//!
//! Live tests are marked `#[ignore]`; run with a running MongoDB at
//! `MONGODB_URL` via:
//! ```text
//! cargo test -p klauthed-data --features mongodb -- --ignored
//! ```

use async_trait::async_trait;
use klauthed_core::time::Timestamp;
use mongodb::Collection;
use mongodb::bson::{Bson, Document, doc, to_bson};
use mongodb::options::{FindOptions, IndexOptions};
use mongodb::{Database, IndexModel};

use crate::error::DataError;
use crate::outbox::{Outbox, OutboxEntry, OutboxId};

/// Default MongoDB collection name for the outbox.
const DEFAULT_COLLECTION: &str = "outbox";

/// A durable [`Outbox`] backed by a MongoDB collection.
///
/// Clone-cheap: holds only the collection handle (an `Arc` internally).
#[derive(Clone)]
pub struct MongoOutbox {
    collection: Collection<Document>,
}

impl MongoOutbox {
    /// Wrap an existing database, using the default collection name `"outbox"`.
    pub fn new(db: &Database) -> Self {
        Self::with_collection(db, DEFAULT_COLLECTION)
    }

    /// Wrap an existing database, using `collection_name` as the target collection.
    pub fn with_collection(db: &Database, collection_name: &str) -> Self {
        Self { collection: db.collection(collection_name) }
    }

    /// Create the indexes required for efficient relay queries.
    ///
    /// Safe to call repeatedly (`create_index` is idempotent when the index
    /// definition has not changed).
    pub async fn ensure_schema(&self) -> Result<(), DataError> {
        // Compound index for the relay poll: unpublished entries sorted by sequence.
        let relay_index = IndexModel::builder()
            .keys(doc! { "published": 1, "sequence": 1 })
            .options(IndexOptions::builder().name(Some("published_sequence".to_owned())).build())
            .build();

        // Sparse index for per-aggregate look-ups.
        let agg_index = IndexModel::builder()
            .keys(doc! { "aggregate_id": 1 })
            .options(
                IndexOptions::builder()
                    .name(Some("aggregate_id".to_owned()))
                    .sparse(Some(true))
                    .build(),
            )
            .build();

        self.collection
            .create_index(relay_index)
            .await
            .map_err(|e| DataError::Outbox(format!("mongo create index failed: {e}")))?;

        self.collection
            .create_index(agg_index)
            .await
            .map_err(|e| DataError::Outbox(format!("mongo create index failed: {e}")))?;

        Ok(())
    }

    /// Borrow the underlying collection handle.
    pub fn collection(&self) -> &Collection<Document> {
        &self.collection
    }
}

/// Serialize an [`OutboxEntry`] to a BSON `Document`.
fn entry_to_doc(entry: &OutboxEntry) -> Result<Document, DataError> {
    let payload = to_bson(&entry.payload)
        .map_err(|e| DataError::Outbox(format!("bson serialization of payload failed: {e}")))?;

    let mut doc = doc! {
        "_id":           entry.id.to_string(),
        "aggregate_type": &entry.aggregate_type,
        "aggregate_id":   &entry.aggregate_id,
        "event_type":     &entry.event_type,
        "sequence":       entry.sequence as i64,
        "payload":        payload,
        "occurred_at":    entry.occurred_at.to_rfc3339(),
        "published":      entry.published,
    };

    match entry.published_at {
        Some(ts) => {
            doc.insert("published_at", ts.to_rfc3339());
        }
        None => {
            doc.insert("published_at", Bson::Null);
        }
    }

    Ok(doc)
}

/// Deserialize a BSON `Document` back into an [`OutboxEntry`].
fn doc_to_entry(doc: &Document) -> Result<OutboxEntry, DataError> {
    let id_str = doc.get_str("_id").map_err(|e| DataError::Outbox(format!("missing _id: {e}")))?;
    let id: OutboxId = id_str
        .parse()
        .map_err(|e| DataError::Outbox(format!("invalid outbox id '{id_str}': {e}")))?;

    let occurred_at_str = doc
        .get_str("occurred_at")
        .map_err(|e| DataError::Outbox(format!("missing occurred_at: {e}")))?;
    let occurred_at = parse_timestamp(occurred_at_str)?;

    let published_at = match doc.get("published_at") {
        Some(Bson::String(s)) => Some(parse_timestamp(s)?),
        _ => None,
    };

    let payload_bson =
        doc.get("payload").ok_or_else(|| DataError::Outbox("missing payload field".to_owned()))?;
    let payload: serde_json::Value = mongodb::bson::from_bson(payload_bson.clone())
        .map_err(|e| DataError::Outbox(format!("bson payload to json failed: {e}")))?;

    let sequence =
        doc.get_i64("sequence").map_err(|e| DataError::Outbox(format!("missing sequence: {e}")))?;

    let published = doc
        .get_bool("published")
        .map_err(|e| DataError::Outbox(format!("missing published: {e}")))?;

    Ok(OutboxEntry {
        id,
        aggregate_type: doc
            .get_str("aggregate_type")
            .map_err(|e| DataError::Outbox(format!("missing aggregate_type: {e}")))?
            .to_owned(),
        aggregate_id: doc
            .get_str("aggregate_id")
            .map_err(|e| DataError::Outbox(format!("missing aggregate_id: {e}")))?
            .to_owned(),
        event_type: doc
            .get_str("event_type")
            .map_err(|e| DataError::Outbox(format!("missing event_type: {e}")))?
            .to_owned(),
        sequence: sequence as u64,
        payload,
        occurred_at,
        published,
        published_at,
    })
}

fn parse_timestamp(s: &str) -> Result<Timestamp, DataError> {
    serde_json::from_value(serde_json::Value::String(s.to_owned()))
        .map_err(|e| DataError::Outbox(format!("invalid stored timestamp '{s}': {e}")))
}

#[async_trait]
impl Outbox for MongoOutbox {
    async fn enqueue(&self, entries: Vec<OutboxEntry>) -> Result<(), DataError> {
        if entries.is_empty() {
            return Ok(());
        }

        let docs: Vec<Document> = entries.iter().map(entry_to_doc).collect::<Result<_, _>>()?;

        self.collection
            .insert_many(docs)
            .await
            .map_err(|e| DataError::Outbox(format!("mongo insert_many failed: {e}")))?;

        Ok(())
    }

    async fn fetch_unpublished(&self, limit: usize) -> Result<Vec<OutboxEntry>, DataError> {
        let filter = doc! { "published": false };
        let options =
            FindOptions::builder().sort(doc! { "sequence": 1 }).limit(Some(limit as i64)).build();

        let mut cursor = self
            .collection
            .find(filter)
            .with_options(options)
            .await
            .map_err(|e| DataError::Outbox(format!("mongo find failed: {e}")))?;

        let mut entries = Vec::new();
        while cursor
            .advance()
            .await
            .map_err(|e| DataError::Outbox(format!("mongo cursor advance failed: {e}")))?
        {
            let doc = cursor
                .deserialize_current()
                .map_err(|e| DataError::Outbox(format!("mongo deserialize failed: {e}")))?;
            entries.push(doc_to_entry(&doc)?);
        }

        Ok(entries)
    }

    async fn mark_published(&self, ids: &[OutboxId]) -> Result<(), DataError> {
        if ids.is_empty() {
            return Ok(());
        }

        let id_strings: Vec<Bson> = ids.iter().map(|id| Bson::String(id.to_string())).collect();

        let now = Timestamp::now().to_rfc3339();
        let filter = doc! { "_id": { "$in": id_strings } };
        let update = doc! {
            "$set": {
                "published": true,
                "published_at": now,
            }
        };

        self.collection
            .update_many(filter, update)
            .await
            .map_err(|e| DataError::Outbox(format!("mongo update_many failed: {e}")))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use klauthed_core::domain::{DomainEvent, EventEnvelope};
    use klauthed_core::id::Id;
    use serde::Serialize;
    use std::borrow::Cow;

    #[derive(Debug, Serialize)]
    struct Opened {
        owner: String,
    }

    impl DomainEvent for Opened {
        fn event_type(&self) -> &'static str {
            "account.opened"
        }
    }

    fn entry(seq: u64) -> OutboxEntry {
        let envelope = EventEnvelope {
            event_id: Id::new(),
            event_type: Cow::Borrowed("account.opened"),
            aggregate_id: "acct-1".to_owned(),
            aggregate_type: Cow::Borrowed("account"),
            sequence: seq,
            occurred_at: Timestamp::from_unix_millis(1_000 + seq as i64),
            payload: Opened { owner: format!("owner-{seq}") },
        };
        OutboxEntry::from_envelope(&envelope).unwrap()
    }

    async fn live_outbox() -> MongoOutbox {
        let url =
            std::env::var("MONGODB_URL").unwrap_or_else(|_| "mongodb://127.0.0.1:27017".to_owned());
        let client = mongodb::Client::with_uri_str(&url).await.expect("connect mongodb");
        let db_name = format!("klauthed_test_{}", Id::<()>::new());
        let db = client.database(&db_name);
        let outbox = MongoOutbox::new(&db);
        outbox.ensure_schema().await.expect("ensure schema");
        outbox
    }

    #[tokio::test]
    #[ignore = "requires a live MongoDB at MONGODB_URL"]
    async fn enqueue_fetch_mark_round_trip() {
        let outbox = live_outbox().await;
        let e1 = entry(1);
        let e2 = entry(2);
        let (id1, id2) = (e1.id, e2.id);

        outbox.enqueue(vec![e1, e2]).await.unwrap();

        let unpublished = outbox.fetch_unpublished(10).await.unwrap();
        assert_eq!(unpublished.len(), 2);

        outbox.mark_published(&[id1]).await.unwrap();
        let remaining = outbox.fetch_unpublished(10).await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, id2);

        outbox.mark_published(&[id2]).await.unwrap();
        assert!(outbox.fetch_unpublished(10).await.unwrap().is_empty());
    }
}
