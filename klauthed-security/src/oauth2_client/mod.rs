//! OAuth 2.0 client registration model.
//!
//! * [`OAuth2Client`] — the server-side registration record.
//! * [`ClientStore`] — async storage trait (implement for SQL, Redis, etc.).
//! * [`InMemoryClientStore`] — in-memory implementation for tests.
//!
//! See the [`store`] and [`client`] sub-modules for details.

pub mod client;
pub mod store;

pub use client::{ClientGrantType, ClientType, OAuth2Client, TokenEndpointAuthMethod};
pub use store::{ClientStore, InMemoryClientStore};
