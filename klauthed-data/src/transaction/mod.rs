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
//! For a real transaction, [`SqlxTransact`] (feature `sql`) and [`MongoTransact`]
//! (feature `mongodb`) begin a transaction and **pass the handle to the closure**
//! — statements only join a transaction when issued on its connection / session —
//! committing on `Ok` and rolling back on `Err`.

#[cfg(feature = "mongodb")]
pub mod mongo;
pub mod noop;
#[cfg(feature = "sql")]
pub mod sql;
pub mod transact;

#[cfg(feature = "mongodb")]
pub use mongo::MongoTransact;
pub use noop::NoopTransact;
#[cfg(feature = "sql")]
pub use sql::SqlxTransact;
pub use transact::Transact;
