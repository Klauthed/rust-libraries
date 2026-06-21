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
//! [`NoopTransact`] simply calls the closure with no transaction semantics —
//! sufficient for verifying business logic without a real database.
//!
//! For a real relational transaction, [`SqlxTransact`] (feature `sql`) begins a
//! sqlx transaction and **passes the handle to the closure** (sqlx statements
//! only join a transaction when they run on its connection), committing on `Ok`
//! and rolling back on `Err`.
//!
//! # Future work
//!
//! * `MongoTransact` — wraps a MongoDB client session for multi-document
//!   atomicity.

pub mod noop;
#[cfg(feature = "sql")]
pub mod sql;
pub mod transact;

pub use noop::NoopTransact;
#[cfg(feature = "sql")]
pub use sql::SqlxTransact;
pub use transact::Transact;
