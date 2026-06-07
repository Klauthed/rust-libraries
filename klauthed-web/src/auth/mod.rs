//! JWT Bearer token authentication middleware and extractors.
//!
//! Three cooperating pieces:
//!
//! * [`JwtAuth`] — an actix [`Transform`] that enforces a valid
//!   `Authorization: Bearer <token>` header on every request in its scope. The
//!   decoded [`Claims`] are stored in request extensions so extractors can read
//!   them. Requests with a missing, malformed, or expired token are rejected
//!   with `401 Unauthorized` before reaching any handler.
//! * [`AuthenticatedUser`] — a [`FromRequest`] extractor that yields the
//!   [`Claims`] from the current request. Returns `401` if no claims are
//!   present (i.e. the middleware was not mounted or the route is outside its
//!   scope). Derefs to [`Claims`] so all accessors are available directly.
//! * [`OptionalAuthentication`] — a [`FromRequest`] extractor that yields
//!   `Option<Claims>`, never failing. Useful for routes that serve both
//!   authenticated and anonymous callers.
//!
//! # Wiring
//!
//! 1. Build your [`JwtVerifier`] (once, shared across workers) and register it
//!    as [`web::Data`]:
//!
//! ```no_run
//! use actix_web::{web, App};
//! use klauthed_security::JwtVerifier;
//! use klauthed_web::auth::JwtAuth;
//!
//! let verifier = JwtVerifier::hs256(b"my-signing-secret");
//! let _app = App::new()
//!     .app_data(web::Data::new(verifier))
//!     .wrap(JwtAuth::new());
//! ```
//!
//! 2. Require auth in handlers by accepting [`AuthenticatedUser`]:
//!
//! ```no_run
//! use actix_web::HttpResponse;
//! use klauthed_web::auth::AuthenticatedUser;
//!
//! async fn protected(user: AuthenticatedUser) -> HttpResponse {
//!     let subject = user.sub().unwrap_or("unknown");
//!     HttpResponse::Ok().body(format!("hello, {subject}"))
//! }
//! ```
//!
//! # Security notes
//!
//! * The `Authorization` prefix must be exactly `"Bearer "` (case-sensitive,
//!   one trailing space). Other schemes (Basic, Digest, …) are rejected with
//!   `401`.
//! * Token expiry (`exp`) and not-before (`nbf`) are enforced by the
//!   [`JwtVerifier`]. Configure [`leeway_seconds`] on the verifier if your
//!   upstream clocks skew.
//! * `5xx` server error messages are never included in responses (actix's
//!   `ResponseError` rendering via [`AppError`] handles this). Configuration
//!   errors (e.g. a missing verifier) produce `500` without leaking details.
//!
//! [`leeway_seconds`]: klauthed_security::JwtVerifier::leeway_seconds
//! [`Transform`]: actix_web::dev::Transform
//! [`FromRequest`]: actix_web::FromRequest
//! [`web::Data`]: actix_web::web::Data
//! [`Claims`]: klauthed_security::Claims
//! [`JwtVerifier`]: klauthed_security::JwtVerifier
//! [`AppError`]: crate::error::AppError

pub mod extractors;
pub mod middleware;

pub use extractors::{AuthenticatedUser, OptionalAuthentication};
pub use middleware::{JwtAuth, JwtAuthService, TokenRevocationCheck};
