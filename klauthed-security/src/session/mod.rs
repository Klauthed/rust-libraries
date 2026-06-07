//! Opaque server-side sessions.
//!
//! A [`Session`] binds a random, unguessable [`SessionId`] to a principal
//! (subject) and an expiry window. Sessions are stored behind the async
//! [`SessionStore`] trait; the bundled [`InMemorySessionStore`] keeps them in a
//! `Mutex<HashMap>` and decides expiry from an injected
//! [`klauthed_core::time::Clock`], so time-based behaviour is testable
//! with [`FixedClock`](klauthed_core::time::FixedClock).
//!
//! Session ids are minted from the same OS-CSPRNG-backed [`random_token`](crate::token::random_token) used
//! everywhere else in the crate (256 bits of entropy, URL-safe base64), so they
//! are safe to place in cookies and headers.
//!
//! # Not (yet) included
//!
//! A persistent (DB/Redis-backed) [`SessionStore`] implementation is future
//! work; this pass ships only the in-memory store. The trait is the stable
//! seam those backends would implement.
//!
//! ```
//! use klauthed_security::session::{InMemorySessionStore, SessionStore};
//! use klauthed_core::time::{FixedClock, Timestamp};
//! use klauthed_core::time::Duration;
//! use std::sync::Arc;
//!
//! # async fn demo() {
//! let clock = Arc::new(FixedClock::at_unix_millis(0));
//! let store = InMemorySessionStore::with_clock(clock.clone());
//!
//! let session = store.create("user-123", Duration::minutes(30), None).await.unwrap();
//! let id = session.id.clone();
//!
//! // Still valid now.
//! assert!(store.get(&id).await.unwrap().is_some());
//!
//! // Advance past expiry: `get` now returns `None`.
//! clock.advance(Duration::hours(1));
//! assert!(store.get(&id).await.unwrap().is_none());
//! # }
//! ```

pub mod model;
pub mod store;

pub use model::{Session, SessionId};
pub use store::{InMemorySessionStore, SessionStore};
