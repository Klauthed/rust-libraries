//! CORS configuration — static and dynamic.
//!
//! Two complementary approaches, both ready to mount with `.wrap(...)`:
//!
//! | Sub-module | Responsibility |
//! |---|---|
//! | [`config`] | [`CorsConfig`] and the static [`build_cors`] middleware |
//! | [`registry`] | [`CorsOriginRegistry`] trait + in-memory / caching impls |
//! | [`middleware`] | the per-request [`DynamicCors`] middleware |
//!
//! ## Static CORS — [`build_cors`]
//!
//! Origins are fixed at startup. Good for services with a known, small set of
//! allowed frontends.
//!
//! ```no_run
//! use actix_web::App;
//! use klauthed_web::cors::{CorsConfig, build_cors};
//!
//! let _app = App::new().wrap(build_cors(&CorsConfig::permissive()));
//! ```
//!
//! ## Dynamic CORS — [`DynamicCors`]
//!
//! Origins are checked at request time via a pluggable [`CorsOriginRegistry`].
//! This is the right approach for a multi-tenant IDP where customers register
//! their own auth-page domains:
//!
//! * Your platform's own frontends live in [`CorsConfig::allowed_origins`]
//!   (checked in O(1) without any I/O).
//! * Every tenant's registered domains are resolved by the registry.
//! * Wrap the registry in [`CachedOriginRegistry`] so each origin is looked
//!   up at most once per TTL window, not on every request.
//!
//! ```no_run
//! use std::sync::Arc;
//! use std::time::Duration;
//! use actix_web::App;
//! use klauthed_web::cors::{
//!     CorsConfig, CachedOriginRegistry, DynamicCors, InMemoryOriginRegistry,
//! };
//!
//! // Your own auth frontend — always allowed, no I/O needed.
//! let config = CorsConfig {
//!     allowed_origins: vec!["https://auth.klauthed.com".into()],
//!     allow_credentials: true,
//!     ..CorsConfig::default()
//! };
//!
//! // Tenant domains fetched from your DB, cached for 5 minutes.
//! // In production, replace InMemoryOriginRegistry with your own
//! // TenantOriginRegistry that queries the tenant table.
//! let registry = Arc::new(CachedOriginRegistry::new(
//!     InMemoryOriginRegistry::new(),
//!     Duration::from_secs(300),
//! ));
//!
//! let cors = DynamicCors::new(config, registry);
//!
//! // Mount CORS *outside* JwtAuth so OPTIONS preflight is answered before
//! // auth checks run.
//! let _app = App::new()
//!     .wrap(klauthed_web::auth::JwtAuth::new())
//!     .wrap(cors);
//! ```
//!
//! ## Security notes
//!
//! * CORS must be mounted **outer** (last in `.wrap()` chain) so it runs
//!   first on incoming requests and handles `OPTIONS` before auth.
//! * Always send `Vary: Origin` when echoing a specific origin — this tells
//!   CDNs and proxies that the response differs per caller. Both middlewares
//!   here do this automatically.
//! * Never combine `Access-Control-Allow-Origin: *` with
//!   `Access-Control-Allow-Credentials: true` — browsers reject it.
//! * Keep allowed headers minimal: exposed headers become readable by
//!   cross-origin JS.

pub mod config;
pub mod middleware;
pub mod registry;

pub use config::{CorsConfig, build_cors};
pub use middleware::{DynamicCors, DynamicCorsService};
pub use registry::{CachedOriginRegistry, CorsOriginRegistry, InMemoryOriginRegistry};
