//! Cross-Site Request Forgery (CSRF) protection.
//!
//! [`Csrf`] is an actix middleware implementing the **double-submit-cookie**
//! pattern: a random token is kept in a JavaScript-readable cookie and must be
//! echoed back in a request header on every state-changing request. Because the
//! token lives nowhere the server has to store, the middleware is stateless.
//!
//! ```no_run
//! use actix_web::App;
//! use klauthed_web::csrf::{Csrf, CsrfConfig};
//!
//! // Defaults: cookie `csrf_token`, header `x-csrf-token`, Bearer requests
//! // skipped, a fresh cookie auto-issued on the first safe request.
//! let _app = App::new().wrap(Csrf::new());
//!
//! // Local HTTP development (no `Secure` attribute):
//! let _dev = App::new().wrap(Csrf::from_config(CsrfConfig::default().secure(false)));
//! ```
//!
//! ## How it works
//!
//! * **Safe** methods (`GET`/`HEAD`/`OPTIONS`/`TRACE`) always pass. With
//!   [`auto_issue`](CsrfConfig::auto_issue), a request without a CSRF cookie gets
//!   one minted and set on the response so the client can begin echoing it.
//! * **Unsafe** methods must present a CSRF cookie *and* a matching header value
//!   (constant-time compared). A mismatch or absence returns `403 Forbidden`.
//! * `Authorization: Bearer` requests are skipped by default — token-based APIs
//!   don't send ambient cookie credentials and so aren't exposed to CSRF.
//!
//! ## Client side
//!
//! Single-page apps read the cookie and send it back as the header on writes:
//!
//! ```js
//! const token = document.cookie.match(/csrf_token=([^;]+)/)?.[1];
//! fetch("/api/thing", { method: "POST", headers: { "x-csrf-token": token } });
//! ```
//!
//! Rotate the token after a privilege change (e.g. login) with
//! [`Csrf::issue_cookie`].

pub mod config;
pub mod middleware;

pub use config::{CsrfConfig, CsrfSameSite};
pub use middleware::{Csrf, CsrfService};
