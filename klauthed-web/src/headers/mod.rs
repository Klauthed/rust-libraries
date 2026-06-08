//! Security response headers.
//!
//! [`SecurityHeaders`] is an actix middleware that adds the standard hardening
//! headers to every response — HSTS, `X-Frame-Options`, `X-Content-Type-Options`,
//! `Referrer-Policy`, `Content-Security-Policy`, and the cross-origin isolation
//! headers — driven by a [`SecurityHeadersConfig`].
//!
//! ```no_run
//! use actix_web::App;
//! use klauthed_web::headers::{SecurityHeaders, SecurityHeadersConfig};
//!
//! // Strict defaults, good for a JSON / auth API.
//! let _api = App::new().wrap(SecurityHeaders::new());
//!
//! // Loosened for an app that also serves HTML pages.
//! let _html = App::new().wrap(SecurityHeaders::from_config(&SecurityHeadersConfig::relaxed()));
//! ```
//!
//! ## Notes
//!
//! * Mount it **outermost** (last in the `.wrap()` chain) so the headers also
//!   cover error responses produced by inner layers.
//! * A header is only added when the handler did not set it, so a single route
//!   can override the policy (e.g. a permissive CSP for an embedded widget).
//! * HSTS is emitted unconditionally; browsers ignore it over plain HTTP. Use
//!   [`SecurityHeadersConfig::without_hsts`] for local `http://` development.

pub mod config;
pub mod middleware;

pub use config::{FrameOptions, Hsts, SecurityHeadersConfig};
pub use middleware::{SecurityHeaders, SecurityHeadersService};
