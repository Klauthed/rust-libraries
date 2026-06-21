//! [`MongoTransact`]: a MongoDB-backed transactional executor.

use mongodb::{Client, ClientSession};

use crate::DataError;

/// Runs a unit of work inside a MongoDB transaction (multi-document atomicity),
/// committing on `Ok` and aborting on `Err`.
///
/// Like [`SqlxTransact`](super::SqlxTransact), it **passes the session handle** to
/// the closure ([`run`](Self::run)) — MongoDB operations join the transaction only
/// when issued with that session. Requires a **replica set**; transactions are
/// unsupported on a standalone `mongod`.
///
/// ```ignore
/// use klauthed_data::{DataError, MongoTransact};
/// use mongodb::bson::doc;
///
/// let tx = MongoTransact::new(client);
/// tx.run(async |session| {
///     accounts
///         .insert_one(doc! { "name": "alice" })
///         .session(&mut *session)
///         .await
///         .map_err(|e| DataError::Transaction(e.to_string()))?;
///     Ok::<_, DataError>(())
/// })
/// .await?;
/// ```
#[derive(Clone)]
pub struct MongoTransact {
    client: Client,
}

impl MongoTransact {
    /// Wrap a client.
    #[must_use]
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    /// Begin a session + transaction, run `work` with the session handle, then
    /// commit on `Ok` or abort on `Err`.
    ///
    /// The abort is best-effort: the closure's original error is returned even if
    /// the abort itself fails (the session ending also aborts an open transaction).
    ///
    /// # Errors
    /// Returns the closure's error (after aborting), or a
    /// [`DataError::Transaction`] if starting the session/transaction or
    /// committing fails.
    pub async fn run<T, E, F>(&self, work: F) -> Result<T, E>
    where
        F: AsyncFnOnce(&mut ClientSession) -> Result<T, E>,
        E: From<DataError>,
    {
        let mut session = self
            .client
            .start_session()
            .await
            .map_err(|e| DataError::Transaction(format!("start session: {e}")))?;
        session
            .start_transaction()
            .await
            .map_err(|e| DataError::Transaction(format!("start transaction: {e}")))?;

        match work(&mut session).await {
            Ok(value) => {
                session
                    .commit_transaction()
                    .await
                    .map_err(|e| DataError::Transaction(format!("commit: {e}")))?;
                Ok(value)
            }
            Err(error) => {
                let _ = session.abort_transaction().await;
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mongodb::bson::{Document, doc};

    // Transactions require a MongoDB *replica set*, so this is ignored by default.
    // Run against one with:
    //   cargo test -p klauthed-data --features mongodb transaction::mongo -- --ignored
    #[tokio::test]
    #[ignore = "requires a running MongoDB replica set"]
    async fn commits_on_ok_and_aborts_on_err() {
        let url = std::env::var("MONGODB_URL")
            .unwrap_or_else(|_| "mongodb://127.0.0.1:27017".to_string());
        let client = Client::with_uri_str(&url).await.expect("connect mongodb");

        // Transactions require a replica set; skip on a standalone deployment
        // (e.g. CI's mongo service container, which can't be a replica set).
        let hello =
            client.database("admin").run_command(doc! { "hello": 1 }).await.expect("hello command");
        if hello.get_str("setName").is_err() {
            eprintln!("skipping MongoTransact test: MongoDB is not a replica set");
            return;
        }

        let collection = client.database("klauthed_test").collection::<Document>("transact_items");
        collection.drop().await.ok();

        let tx = MongoTransact::new(client.clone());

        // Commit path: the insert persists.
        let committed: Result<(), DataError> = tx
            .run(async |session| {
                collection
                    .insert_one(doc! { "name": "a" })
                    .session(&mut *session)
                    .await
                    .map_err(|e| DataError::Transaction(e.to_string()))?;
                Ok(())
            })
            .await;
        assert!(committed.is_ok());
        assert_eq!(collection.count_documents(doc! {}).await.unwrap(), 1);

        // Abort path: the insert is rolled back.
        let aborted: Result<(), DataError> = tx
            .run(async |session| {
                collection
                    .insert_one(doc! { "name": "b" })
                    .session(&mut *session)
                    .await
                    .map_err(|e| DataError::Transaction(e.to_string()))?;
                Err(DataError::Transaction("forced abort".into()))
            })
            .await;
        assert!(aborted.is_err());
        assert_eq!(
            collection.count_documents(doc! {}).await.unwrap(),
            1,
            "the aborted insert must not persist"
        );
    }
}
