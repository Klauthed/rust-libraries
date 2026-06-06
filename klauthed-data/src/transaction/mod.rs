//! Unit-of-work transaction abstraction.
//!
//! A [`Transact`] implementation wraps an operation in a database transaction
//! and automatically rolls it back on failure.
//!
//! # Design
//!
//! The trait is *not* generic over the connection type — operations inside the
//! transaction receive the underlying connection/pool through their own
//! dependency injection. This keeps the trait object-safe and avoids leaking
//! driver-specific types into business logic.
//!
//! In production, implementations commit on `Ok` and roll back on `Err`.
//! In tests, [`NoopTransact`] simply calls the closure with no transaction
//! semantics — sufficient for verifying business logic without a real database.
//!
//! # Future work
//!
//! * `SqlxTransact<DB>` — wraps a `sqlx::Pool<DB>`, begins a transaction, and
//!   provides the `Transaction` handle to the closure.
//! * `MongoTransact` — wraps a MongoDB client session for multi-document
//!   atomicity.

use std::future::Future;

use crate::DataError;

// ── Transact ──────────────────────────────────────────────────────────────────

/// Execute a unit of work atomically.
///
/// Implementations must:
/// * Begin a transaction before calling `f`.
/// * Commit on `Ok(value)`.
/// * Roll back on `Err(e)` and propagate the error.
///
/// The closure `f` returns a `Future` so async operations (queries, lock
/// acquisitions, etc.) can be composed within the transaction boundary.
///
/// # Object safety
///
/// This trait is **not** object-safe because `F` is generic. Use a concrete
/// implementation stored as a value (not `dyn Transact`) or wrap it in a
/// type-erased adapter.
pub trait Transact {
    /// Run `f` inside a transaction, committing on success and rolling back on
    /// failure.
    ///
    /// # Errors
    /// Returns the error produced by `f`, or a backend error if the commit /
    /// rollback itself fails.
    fn transact<F, Fut, T, E>(&self, f: F) -> impl Future<Output = Result<T, E>>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, E>>,
        E: From<DataError>;
}

// ── NoopTransact ──────────────────────────────────────────────────────────────

/// A no-op [`Transact`] implementation for unit tests.
///
/// Calls the closure directly with no transaction semantics. Use this when
/// you want to test business logic that calls into a `Transact` without
/// needing a real database.
///
/// ```
/// use klauthed_data::transaction::{NoopTransact, Transact};
/// use klauthed_data::DataError;
///
/// # #[tokio::main]
/// # async fn main() {
/// let tx = NoopTransact;
///
/// let result: Result<i32, DataError> = tx.transact(|| async { Ok(42) }).await;
/// assert_eq!(result.unwrap(), 42);
///
/// let err: Result<i32, DataError> = tx
///     .transact(|| async { Err(DataError::InvalidPage("bad".into())) })
///     .await;
/// assert!(err.is_err());
/// # }
/// ```
pub struct NoopTransact;

impl Transact for NoopTransact {
    async fn transact<F, Fut, T, E>(&self, f: F) -> Result<T, E>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, E>>,
        E: From<DataError>,
    {
        f().await
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn noop_passes_through_ok() {
        let tx = NoopTransact;
        let result: Result<u32, DataError> = tx.transact(|| async { Ok(99) }).await;
        assert_eq!(result.unwrap(), 99);
    }

    #[tokio::test]
    async fn noop_propagates_error() {
        let tx = NoopTransact;
        let result: Result<u32, DataError> =
            tx.transact(|| async { Err(DataError::InvalidPage("test".into())) }).await;
        assert!(matches!(result, Err(DataError::InvalidPage(_))));
    }

    #[tokio::test]
    async fn noop_supports_async_work_inside() {
        let tx = NoopTransact;
        let result: Result<Vec<u32>, DataError> = tx
            .transact(|| async {
                // Simulate async work composed inside the transaction.
                let a = async { 1u32 }.await;
                let b = async { 2u32 }.await;
                Ok(vec![a, b])
            })
            .await;
        assert_eq!(result.unwrap(), vec![1, 2]);
    }
}
