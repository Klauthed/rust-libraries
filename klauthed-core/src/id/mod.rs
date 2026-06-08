//! Typed 128-bit identifiers.
//!
//! [`Id<T>`] is a phantom-typed newtype over a [`Uuid`](uuid::Uuid), so `Id<User>` and
//! `Id<Order>` are distinct types the compiler refuses to mix. The default
//! generator is **UUID v7** (time-sortable); v4 and ULID generation are also
//! available. Because all three encode the same 128 bits, one id can be rendered
//! and parsed as either a UUID or a ULID string.
//!
//! ```
//! use klauthed_core::id::Id;
//!
//! struct User;
//! type UserId = Id<User>;
//!
//! let a = UserId::new();        // UUID v7, time-sortable
//! let b = UserId::new();
//! assert!(a != b);
//! // round-trips through both string forms:
//! assert_eq!(a, a.to_string().parse().unwrap());
//! assert_eq!(a, UserId::from_ulid_str(&a.to_ulid_string()).unwrap());
//! ```

pub mod error;
pub mod typed;

pub use error::ParseIdError;
pub use typed::Id;
