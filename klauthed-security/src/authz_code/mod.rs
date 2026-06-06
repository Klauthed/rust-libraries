//! OAuth 2.0 authorization code storage (RFC 6749 §4.1).
//!
//! Three focused sub-modules:
//!
//! * [`code`] — [`AuthCode`], [`AuthCodeBuilder`], [`PkceMethod`]
//! * [`store`] — [`AuthCodeStore`] trait and [`InMemoryAuthCodeStore`]
//! * [`pkce`]  — [`verify_pkce`] PKCE verifier (RFC 7636)
//!
//! # Typical flow
//!
//! ```
//! use std::sync::Arc;
//! use klauthed_core::time::FixedClock;
//! use klauthed_security::authz_code::{AuthCodeBuilder, AuthCodeStore, InMemoryAuthCodeStore};
//!
//! # #[tokio::main]
//! # async fn main() {
//! let clock = Arc::new(FixedClock::at_unix_millis(0));
//! let store = InMemoryAuthCodeStore::with_clock(clock.clone());
//!
//! let code = AuthCodeBuilder::new("client-id", "user-sub")
//!     .redirect_uri("https://app.example.com/cb")
//!     .scope(vec!["openid".into()])
//!     .build(&*clock, chrono::Duration::minutes(5))
//!     .unwrap();
//!
//! let code_str = code.code.clone();
//! store.store(code).await.unwrap();
//!
//! // Single-use: second consume returns None.
//! assert!(store.consume(&code_str).await.unwrap().is_some());
//! assert!(store.consume(&code_str).await.unwrap().is_none());
//! # }
//! ```

pub mod code;
pub mod pkce;
pub mod store;

pub use code::{AuthCode, AuthCodeBuilder, PkceMethod};
pub use pkce::verify_pkce;
pub use store::{AuthCodeStore, InMemoryAuthCodeStore};
