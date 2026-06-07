//! Per-request [`RequestContext`](klauthed_core::context::RequestContext) plumbing for actix-web.
//!
//! This module provides two pieces that work together:
//!
//! * [`RequestContextMiddleware`] — an actix [`Transform`](actix_web::dev::Transform) that, for every
//!   request, builds a [`RequestContext`](klauthed_core::context::RequestContext) from inbound headers
//!   (`x-request-id`, `x-correlation-id`, `x-tenant-id`, `Accept-Language`),
//!   stores it in the request extensions, and echoes the resolved request id
//!   back on the response as `x-request-id`. With the `context-scope` feature
//!   enabled, it also installs the context as the ambient
//!   [`RequestContext::current`](klauthed_core::context::RequestContext) for the handler future.
//! * [`Context`] — a [`FromRequest`](actix_web::FromRequest) extractor that hands the stored
//!   [`RequestContext`](klauthed_core::context::RequestContext) to handlers (`async fn handler(ctx: Context)`),
//!   falling back to a fresh default if, for some reason, none is present.
//!
//! ```no_run
//! use actix_web::{web, App, HttpResponse};
//! use klauthed_web::context::{Context, RequestContextMiddleware};
//!
//! async fn handler(ctx: Context) -> HttpResponse {
//!     HttpResponse::Ok().body(ctx.request_id().to_string())
//! }
//!
//! let app = App::new()
//!     .wrap(RequestContextMiddleware::new())
//!     .route("/", web::get().to(handler));
//! ```

pub mod extractor;
pub mod middleware;

pub use extractor::Context;
pub use middleware::{RequestContextMiddleware, RequestContextService};

/// Header carrying the request id (generated when absent).
pub const REQUEST_ID_HEADER: &str = "x-request-id";
/// Header carrying an inbound correlation / trace id.
pub const CORRELATION_ID_HEADER: &str = "x-correlation-id";
/// Header carrying the tenant identifier.
pub const TENANT_ID_HEADER: &str = "x-tenant-id";
/// Standard header used to derive the request locale.
pub const ACCEPT_LANGUAGE_HEADER: &str = "accept-language";
