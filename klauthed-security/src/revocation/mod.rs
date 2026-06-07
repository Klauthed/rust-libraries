//! Token revocation: a denylist for JWT `jti` values.
//!
//! When a token is revoked (user logout, credential rotation, compromise
//! detection), its `jti` claim is inserted into the [`TokenDenylist`] with the
//! token's original `exp` as the expiry. The denylist self-prunes: once a
//! token's natural lifetime has passed, its entry is lazily evicted on the next
//! [`is_revoked`](TokenDenylist::is_revoked) call because an expired token
//! would fail `exp` validation in [`JwtVerifier`](crate::JwtVerifier) anyway.
//!
//! [`TokenDenylist`] is the async storage trait.
//! [`InMemoryTokenDenylist`] provides a clock-injected, in-memory
//! implementation suitable for tests and single-replica deployments.
//!
//! # Integration
//!
//! After decoding a token with [`JwtVerifier::decode`](crate::JwtVerifier::decode),
//! check the `jti` claim against the denylist before admitting the request:
//!
//! ```
//! use std::sync::Arc;
//! use klauthed_core::time::{FixedClock, Timestamp};
//! use klauthed_security::revocation::{InMemoryTokenDenylist, TokenDenylist};
//!
//! # #[tokio::main]
//! # async fn main() {
//! let denylist = InMemoryTokenDenylist::new();
//! let jti = "unique-token-id-abc";
//! let expires_at = Timestamp::from_unix_millis(9_999_999_999_000); // ~year 2286
//!
//! // Token is live.
//! assert!(!denylist.is_revoked(jti).await.unwrap());
//!
//! // Revoke it.
//! denylist.revoke(jti.into(), expires_at).await.unwrap();
//! assert!(denylist.is_revoked(jti).await.unwrap());
//! # }
//! ```

pub mod denylist;
pub mod memory;

pub use denylist::TokenDenylist;
pub use memory::InMemoryTokenDenylist;
