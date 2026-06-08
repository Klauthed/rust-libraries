//! The [`Transact`] unit-of-work trait.

use std::future::Future;

use crate::DataError;

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
