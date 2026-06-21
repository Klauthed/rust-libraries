//! [`SqlxTransact`]: a sqlx-backed transactional executor.

use sqlx::{Any, AnyPool, Transaction};

use crate::DataError;

/// Runs a unit of work inside a sqlx transaction over an [`AnyPool`], committing
/// on `Ok` and rolling back on `Err`.
///
/// Unlike the connection-less [`Transact`](super::Transact) trait, this **passes
/// the live transaction handle to the closure** ([`run`](Self::run)): sqlx
/// statements only participate in a transaction when they execute on that
/// transaction's connection, so the work has to receive it. (`Transact` suits
/// [`NoopTransact`](super::NoopTransact) and flows whose operations bind to the
/// connection through their own dependency injection.)
///
/// ```ignore
/// use klauthed_data::{DataError, SqlxTransact};
///
/// let tx = SqlxTransact::new(pool);
/// let id = tx
///     .run(async |conn| {
///         sqlx::query("INSERT INTO accounts (name) VALUES ('alice')")
///             .execute(&mut **conn)
///             .await?;
///         let (id,): (i64,) = sqlx::query_as("SELECT last_insert_rowid()")
///             .fetch_one(&mut **conn)
///             .await?;
///         Ok::<_, DataError>(id)
///     })
///     .await?;
/// ```
#[derive(Clone)]
pub struct SqlxTransact {
    pool: AnyPool,
}

impl SqlxTransact {
    /// Wrap a connection pool.
    #[must_use]
    pub fn new(pool: AnyPool) -> Self {
        Self { pool }
    }

    /// Begin a transaction, run `work` with the transaction handle, then commit
    /// if it returns `Ok` or roll back if it returns `Err`.
    ///
    /// The rollback is best-effort: the closure's original error is returned even
    /// if the rollback itself fails (the transaction is dropped, which also rolls
    /// back).
    ///
    /// # Errors
    /// Returns the closure's error (after rolling back), or a [`DataError`]-derived
    /// error if beginning or committing the transaction fails.
    pub async fn run<T, E, F>(&self, work: F) -> Result<T, E>
    where
        F: AsyncFnOnce(&mut Transaction<'_, Any>) -> Result<T, E>,
        E: From<DataError>,
    {
        let mut tx = self.pool.begin().await.map_err(DataError::from)?;
        match work(&mut tx).await {
            Ok(value) => {
                tx.commit().await.map_err(DataError::from)?;
                Ok(value)
            }
            Err(error) => {
                let _ = tx.rollback().await;
                Err(error)
            }
        }
    }
}

#[cfg(all(test, feature = "sqlite"))]
mod tests {
    use super::*;
    use sqlx::any::AnyPoolOptions;

    /// A shared in-memory SQLite pool (single connection so the `:memory:` db is
    /// shared across operations) with a simple table.
    async fn memory_pool() -> AnyPool {
        sqlx::any::install_default_drivers();
        let pool = AnyPoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect sqlite memory");
        sqlx::query(
            "CREATE TABLE items (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL)",
        )
        .execute(&pool)
        .await
        .expect("create table");
        pool
    }

    async fn item_count(pool: &AnyPool) -> i64 {
        let (count,): (i64,) =
            sqlx::query_as("SELECT count(*) FROM items").fetch_one(pool).await.expect("count");
        count
    }

    #[tokio::test]
    async fn commits_on_ok() {
        let pool = memory_pool().await;
        let tx = SqlxTransact::new(pool.clone());

        let result: Result<(), DataError> = tx
            .run(async |conn| {
                sqlx::query("INSERT INTO items (name) VALUES ('a')").execute(&mut **conn).await?;
                Ok(())
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(item_count(&pool).await, 1);
    }

    #[tokio::test]
    async fn rolls_back_on_err() {
        let pool = memory_pool().await;
        let tx = SqlxTransact::new(pool.clone());

        let result: Result<(), DataError> = tx
            .run(async |conn| {
                sqlx::query("INSERT INTO items (name) VALUES ('b')").execute(&mut **conn).await?;
                // Returning Err must roll the insert back.
                Err(DataError::Outbox("forced rollback".into()))
            })
            .await;

        assert!(result.is_err());
        assert_eq!(item_count(&pool).await, 0, "the insert must be rolled back");
    }

    #[tokio::test]
    async fn returns_a_value_from_the_closure() {
        let pool = memory_pool().await;
        let tx = SqlxTransact::new(pool.clone());

        let id: i64 = tx
            .run(async |conn| {
                sqlx::query("INSERT INTO items (name) VALUES ('c')").execute(&mut **conn).await?;
                let (id,): (i64,) = sqlx::query_as("SELECT id FROM items WHERE name = 'c'")
                    .fetch_one(&mut **conn)
                    .await?;
                Ok::<_, DataError>(id)
            })
            .await
            .expect("transaction succeeds");

        assert!(id > 0);
        assert_eq!(item_count(&pool).await, 1);
    }
}
