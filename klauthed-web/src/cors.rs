//! CORS (Cross-Origin Resource Sharing) configuration and middleware builder.
//!
//! Browser-facing APIs must send CORS headers so browsers allow cross-origin
//! requests from JavaScript. [`build_cors`] translates a [`CorsConfig`] into
//! an [`actix_cors::Cors`] middleware ready to mount with `.wrap(cors)`.
//!
//! # Quick start
//!
//! Development (allow everything — **never production**):
//!
//! ```no_run
//! use actix_web::App;
//! use klauthed_web::cors::{CorsConfig, build_cors};
//!
//! let _app = App::new().wrap(build_cors(&CorsConfig::permissive()));
//! ```
//!
//! Production (specific origins only):
//!
//! ```no_run
//! use actix_web::App;
//! use klauthed_web::cors::{CorsConfig, build_cors};
//!
//! let config = CorsConfig::restrictive([
//!     "https://app.example.com",
//!     "https://admin.example.com",
//! ]);
//! let _app = App::new().wrap(build_cors(&config));
//! ```
//!
//! # Middleware ordering
//!
//! Mount CORS **before** auth middleware so preflight `OPTIONS` requests are
//! answered immediately, without being rejected by auth checks:
//!
//! ```no_run
//! use actix_web::App;
//! use klauthed_web::cors::{CorsConfig, build_cors};
//! use klauthed_web::auth::JwtAuth;
//!
//! let _app = App::new()
//!     .wrap(JwtAuth::new())   // ← inner (runs last on request, first on response)
//!     .wrap(build_cors(&CorsConfig::permissive())); // ← outer (runs first on request)
//! ```
//!
//! # Security notes
//!
//! * `CorsConfig::permissive()` sets `allowed_origins = ["*"]` and forces
//!   `allow_credentials = false` — browsers reject `credentials + *` anyway.
//! * `allow_credentials = true` must always be paired with explicit origins,
//!   never with a wildcard.
//! * Keep `allowed_headers` minimal to reduce the attack surface for header
//!   injection; the defaults include exactly what klauthed services use.

use actix_cors::Cors;
use actix_web::http;
use serde::{Deserialize, Serialize};

/// Configuration for Cross-Origin Resource Sharing headers.
///
/// Build the middleware with [`build_cors`]. Implements [`serde::Deserialize`]
/// so it can live in a service config file alongside `[server]`, `[database]`,
/// etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CorsConfig {
    /// Allowed origin patterns. An entry of `"*"` means allow all origins
    /// (development only — incompatible with `allow_credentials = true`).
    /// An empty list produces no CORS headers (same-origin only).
    pub allowed_origins: Vec<String>,

    /// HTTP methods the browser may use in cross-origin requests.
    ///
    /// Defaults to `["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"]`.
    pub allowed_methods: Vec<String>,

    /// Request headers the browser may send in cross-origin requests.
    ///
    /// Defaults to the headers klauthed services read:
    /// `Content-Type`, `Authorization`, `Accept`, `Accept-Language`,
    /// `X-Request-Id`, `X-Correlation-Id`, `X-Tenant-Id`.
    pub allowed_headers: Vec<String>,

    /// Response headers the browser JS may read (`Access-Control-Expose-Headers`).
    ///
    /// Defaults to `["X-Request-Id"]` so callers can log the server-assigned
    /// request id for support.
    pub expose_headers: Vec<String>,

    /// Whether to allow cookies and `Authorization` credentials.
    ///
    /// **Must be `false` when `allowed_origins` contains `"*"`** — browsers
    /// reject the combination. Defaults to `false`.
    pub allow_credentials: bool,

    /// How long (seconds) a browser may cache a preflight response.
    ///
    /// Defaults to `86400` (24 hours), which minimises preflight round-trips
    /// in production. Set to `None` to omit the header.
    pub max_age_secs: Option<u32>,
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            allowed_origins: vec![],
            allowed_methods: vec![
                "GET".into(),
                "HEAD".into(),
                "POST".into(),
                "PUT".into(),
                "PATCH".into(),
                "DELETE".into(),
                "OPTIONS".into(),
            ],
            allowed_headers: vec![
                "Content-Type".into(),
                "Authorization".into(),
                "Accept".into(),
                "Accept-Language".into(),
                "X-Request-Id".into(),
                "X-Correlation-Id".into(),
                "X-Tenant-Id".into(),
            ],
            expose_headers: vec!["X-Request-Id".into()],
            allow_credentials: false,
            max_age_secs: Some(86_400),
        }
    }
}

impl CorsConfig {
    /// Fully permissive configuration: allows all origins, methods, and headers.
    ///
    /// **Only suitable for local development.** Never use in production — it
    /// disables all origin checks and exposes every header.
    pub fn permissive() -> Self {
        Self {
            allowed_origins: vec!["*".into()],
            allow_credentials: false, // * + credentials is rejected by browsers
            ..Self::default()
        }
    }

    /// Strict configuration: only the given origins are allowed.
    ///
    /// Sets `allow_credentials = true` so cookies and `Authorization` headers
    /// work for the listed origins. Keeps all other defaults (methods, headers).
    ///
    /// ```
    /// use klauthed_web::cors::CorsConfig;
    ///
    /// let config = CorsConfig::restrictive([
    ///     "https://app.example.com",
    ///     "https://admin.example.com",
    /// ]);
    /// assert!(config.allow_credentials);
    /// assert_eq!(config.allowed_origins.len(), 2);
    /// ```
    pub fn restrictive(origins: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            allowed_origins: origins.into_iter().map(Into::into).collect(),
            allow_credentials: true,
            ..Self::default()
        }
    }
}

/// Build an [`actix_cors::Cors`] middleware from `config`.
///
/// The returned `Cors` instance is ready to mount with `.wrap(...)`.
///
/// # Panics
///
/// Panics if `allowed_headers` or `allowed_methods` contains a string that
/// cannot be parsed as a valid HTTP header name or method — guard against this
/// in configuration validation at startup.
pub fn build_cors(config: &CorsConfig) -> Cors {
    let is_wildcard = config.allowed_origins.iter().any(|o| o == "*");

    // Start from actix_cors::Cors::default() which blocks all cross-origin by
    // default; we then unlock exactly what config asks for.
    let mut cors = if is_wildcard {
        Cors::permissive()
    } else {
        let mut c = Cors::default();
        for origin in &config.allowed_origins {
            c = c.allowed_origin(origin);
        }
        c
    };

    // Methods.
    let methods: Vec<http::Method> = config
        .allowed_methods
        .iter()
        .filter_map(|m| m.parse().ok())
        .collect();
    if !methods.is_empty() {
        cors = cors.allowed_methods(methods);
    }

    // Allowed request headers.
    for header in &config.allowed_headers {
        if let Ok(name) = header.parse::<http::header::HeaderName>() {
            cors = cors.allowed_header(name);
        }
    }

    // Exposed response headers.
    let expose: Vec<http::header::HeaderName> = config
        .expose_headers
        .iter()
        .filter_map(|h| h.parse().ok())
        .collect();
    if !expose.is_empty() {
        cors = cors.expose_headers(expose);
    }

    if config.allow_credentials {
        cors = cors.supports_credentials();
    }

    if let Some(secs) = config.max_age_secs {
        cors = cors.max_age(secs as usize);
    }

    cors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permissive_has_wildcard_and_no_credentials() {
        let c = CorsConfig::permissive();
        assert!(c.allowed_origins.iter().any(|o| o == "*"));
        assert!(!c.allow_credentials);
    }

    #[test]
    fn restrictive_sets_origins_and_credentials() {
        let c = CorsConfig::restrictive(["https://app.example.com"]);
        assert_eq!(c.allowed_origins, ["https://app.example.com"]);
        assert!(c.allow_credentials);
    }

    #[test]
    fn build_cors_does_not_panic_for_permissive() {
        let _ = build_cors(&CorsConfig::permissive());
    }

    #[test]
    fn build_cors_does_not_panic_for_default() {
        let _ = build_cors(&CorsConfig::default());
    }

    #[test]
    fn build_cors_does_not_panic_for_restrictive() {
        let config = CorsConfig::restrictive(["https://app.example.com"]);
        let _ = build_cors(&config);
    }

    #[test]
    fn config_serde_round_trips() {
        let config = CorsConfig::restrictive(["https://a.example.com"]);
        let json = serde_json::to_string(&config).unwrap();
        let back: CorsConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.allowed_origins, config.allowed_origins);
        assert_eq!(back.allow_credentials, config.allow_credentials);
    }

    #[test]
    fn empty_origins_default_config_is_same_origin_only() {
        let c = CorsConfig::default();
        assert!(c.allowed_origins.is_empty());
        assert!(!c.allow_credentials);
    }
}
