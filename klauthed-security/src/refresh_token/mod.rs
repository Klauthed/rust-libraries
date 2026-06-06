//! Refresh token model and storage.
//!
//! * [`RefreshToken`] / [`RefreshTokenBuilder`] — the long-lived credential.
//! * [`RefreshTokenStore`] — async storage trait.
//! * [`InMemoryRefreshTokenStore`] — in-memory implementation with
//!   **token-family replay detection**: presenting a previously consumed token
//!   within its natural lifetime triggers a family-wide revocation.
//! * [`ConsumeResult`] — the four outcomes of a consume call.

pub mod store;
pub mod token;

pub use store::{ConsumeResult, InMemoryRefreshTokenStore, RefreshTokenStore};
pub use token::{RefreshToken, RefreshTokenBuilder};
