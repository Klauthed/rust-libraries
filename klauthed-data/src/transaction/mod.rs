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

pub mod noop;
pub mod transact;

pub use noop::NoopTransact;
pub use transact::Transact;
