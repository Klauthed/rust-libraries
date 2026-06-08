#![deny(unsafe_code)]
#![deny(missing_docs)]
#![cfg_attr(not(test), warn(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

//! Test utilities for klauthed services.
//!
//! A small, focused toolkit that services pull in as a **dev-dependency** to make
//! unit and integration tests deterministic and terse. It builds directly on the
//! `klauthed-core` primitives, so test fixtures use the same types as production
//! code.
//!
//! * [`clock`] — a [`FixedClock`] you can pin and advance, plus re-exports of
//!   [`Clock`] / [`Timestamp`].
//! * [`ids`] — deterministic [`Id<T>`](klauthed_core::id::Id) fixtures from a `u64`
//!   seed ([`seeded_id`], [`nil_id`]).
//! * [`context`] — a deterministic
//!   [`RequestContext`](klauthed_core::context::RequestContext) builder
//!   ([`test_context`], [`TestContextBuilder`]).
//! * [`repository`] — a thread-safe in-memory
//!   [`Repository`](klauthed_core::domain::Repository) ([`InMemoryRepository`]).
//! * [`assertions`] — terse [`DomainError`](klauthed_error::DomainError) assertions
//!   ([`assert_category`], [`assert_code`], and the [`DomainErrorExt`] trait).
//!
//! The most-used items are re-exported at the crate root for convenience.
//!
//! Out of scope for this first cut (possible future work): mock HTTP servers,
//! testcontainers / database harnesses, and clock-driven async test runners.
//!
//! ```
//! use klauthed_testing::{fixed_clock, seeded_id, test_context, Clock};
//! use klauthed_core::id::Id;
//!
//! struct User;
//!
//! let clock = fixed_clock(1_700_000_000_000);
//! let user_id: Id<User> = seeded_id(7);
//! let ctx = test_context();
//!
//! assert_eq!(clock.now().unix_millis(), 1_700_000_000_000);
//! assert_eq!(user_id, seeded_id::<User>(7));
//! assert_eq!(ctx.request_id(), test_context().request_id());
//! ```

pub mod assertions;
pub mod clock;
pub mod context;
pub mod error;
pub mod ids;
pub mod repository;

pub use assertions::{
    DomainErrorExt, assert_category, assert_code, assert_http_status, assert_retryable,
};
pub use clock::{Clock, FixedClock, Timestamp, epoch_clock, fixed_clock};
pub use context::{TestContextBuilder, test_context};
pub use error::TestingError;
pub use ids::{nil_id, seeded_id};
pub use repository::InMemoryRepository;
