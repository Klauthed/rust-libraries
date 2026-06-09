#![deny(unsafe_code)]
#![deny(missing_docs)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)
)]

//! HTTP layer for klauthed services (actix-web).
//!
//! This crate provides the plumbing every klauthed service shares:
//!
//! * [`error`] — [`AppError`], the aggregate error handlers return. It absorbs
//!   any [`klauthed_error::DomainError`] (config, data, …) and renders a uniform
//!   HTTP response via actix's `ResponseError`.
//! * [`context`] — [`RequestContextMiddleware`] establishes a
//!   [`klauthed_core::context::RequestContext`] per request from inbound headers
//!   and the [`Context`] extractor hands it to handlers.
//! * [`health`] — liveness/readiness endpoints with a pluggable
//!   [`HealthCheck`] registry.
//! * [`server`] — bind an actix [`HttpServer`](actix_web::HttpServer) from a
//!   [`ServerConfig`](klauthed_core::config::ServerConfig), optionally pre-wiring
//!   the request-context middleware and health endpoints.
//! * [`ratelimit`] — an in-memory fixed-window [`RateLimit`] middleware that
//!   rejects over-quota clients with `429` and a `Retry-After` header.
//! * [`headers`] — [`SecurityHeaders`] middleware that adds the standard
//!   hardening response headers (HSTS, CSP, `X-Frame-Options`, …).
//! * [`csrf`] — [`Csrf`] double-submit-cookie middleware guarding state-changing
//!   requests against cross-site request forgery.
//! * [`extract`] — [`Json`] and [`Validated`] body extractors that surface
//!   deserialization and [`Validate`](klauthed_core::validation::Validate)
//!   failures as [`AppError`]s.
//!
//! # Wiring it up
//!
//! ```no_run
//! use actix_web::{web, App};
//! use klauthed_web::context::RequestContextMiddleware;
//! use klauthed_web::health::{self, HealthRegistry};
//!
//! let app = App::new()
//!     .wrap(RequestContextMiddleware::new())
//!     .app_data(web::Data::new(HealthRegistry::new()))
//!     .configure(health::configure);
//! ```
//!
//! # Features
//!
//! * `context-scope` — additionally installs the per-request context as the
//!   ambient [`RequestContext::current`](klauthed_core::context::RequestContext)
//!   (via the core `task-local` feature) for the handler future.
//!
//! # Out of scope (future passes)
//!
//! TLS termination, distributed rate limiting, and OpenAPI generation are
//! intentionally not handled here yet.

pub mod app;
pub mod auth;
pub mod context;
pub mod cors;
pub mod csrf;
pub mod error;
pub mod extract;
pub mod headers;
pub mod health; // folder: health/{status,registry,routes,checks}.rs
pub mod oauth; // folder: oauth/{config,util,handlers}.rs
pub mod ratelimit;
pub mod server;
pub mod starter;

pub use app::Components;
pub use auth::{AuthenticatedUser, JwtAuth, OptionalAuthentication, TokenRevocationCheck};
pub use context::{Context, RequestContextMiddleware};
pub use cors::{
    CachedOriginRegistry, CorsConfig, CorsOriginRegistry, DynamicCors, InMemoryOriginRegistry,
    build_cors,
};
pub use csrf::{Csrf, CsrfConfig, CsrfSameSite};
pub use error::{AppError, AppResult};
pub use extract::{Json, Validated};
pub use headers::{FrameOptions, Hsts, SecurityHeaders, SecurityHeadersConfig};
pub use health::{HealthCheck, HealthRegistry, HealthStatus};
pub use ratelimit::{KeyBy, RateLimit};
pub use server::{serve, serve_with_components, serve_with_defaults};
pub use starter::WebStarter;
