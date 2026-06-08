//! The [`NoopTransact`] test double.

use std::future::Future;

use super::Transact;
use crate::DataError;

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
