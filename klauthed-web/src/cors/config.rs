//! Static CORS configuration and the [`actix_cors`]-backed [`build_cors`].

use actix_cors::Cors;
use serde::{Deserialize, Serialize};

/// Configuration for Cross-Origin Resource Sharing headers.
///
/// Used by both [`build_cors`] (static middleware) and
/// [`DynamicCors`](super::DynamicCors) (as the policy + platform-level static
/// origins). Implements [`serde::Deserialize`] so it can live in a service
/// config file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CorsConfig {
    /// Allowed origin patterns. An entry of `"*"` means allow all origins
    /// (development only — incompatible with `allow_credentials = true`).
    /// An empty list produces no CORS headers (same-origin only).
    ///
    /// In [`DynamicCors`](super::DynamicCors) these are the *platform* origins
    /// that are always allowed without any registry lookup.
    pub allowed_origins: Vec<String>,

    /// HTTP methods allowed in cross-origin requests.
    pub allowed_methods: Vec<String>,

    /// Request headers allowed in cross-origin requests.
    pub allowed_headers: Vec<String>,

    /// Response headers exposed to browser JavaScript.
    pub expose_headers: Vec<String>,

    /// Whether to allow cookies and `Authorization` credentials.
    /// Must be `false` when `allowed_origins` contains `"*"`.
    pub allow_credentials: bool,

    /// Preflight cache lifetime in seconds (`None` omits the header).
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
    /// Fully permissive (wildcard). **Development only — never production.**
    pub fn permissive() -> Self {
        Self { allowed_origins: vec!["*".into()], allow_credentials: false, ..Self::default() }
    }

    /// Explicit origins with credentials enabled — production-ready.
    ///
    /// ```
    /// use klauthed_web::cors::CorsConfig;
    ///
    /// let c = CorsConfig::restrictive(["https://app.example.com"]);
    /// assert!(c.allow_credentials);
    /// ```
    pub fn restrictive(origins: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            allowed_origins: origins.into_iter().map(Into::into).collect(),
            allow_credentials: true,
            ..Self::default()
        }
    }
}

/// Build an [`actix_cors::Cors`] middleware from `config` (static origins).
///
/// For tenant-registered dynamic origins use [`DynamicCors`](super::DynamicCors)
/// instead.
pub fn build_cors(config: &CorsConfig) -> Cors {
    let is_wildcard = config.allowed_origins.iter().any(|o| o == "*");

    let mut cors = if is_wildcard {
        Cors::permissive()
    } else {
        let mut c = Cors::default();
        for origin in &config.allowed_origins {
            c = c.allowed_origin(origin);
        }
        c
    };

    let methods: Vec<actix_web::http::Method> =
        config.allowed_methods.iter().filter_map(|m| m.parse().ok()).collect();
    if !methods.is_empty() {
        cors = cors.allowed_methods(methods);
    }

    for header in &config.allowed_headers {
        if let Ok(name) = header.parse::<actix_web::http::header::HeaderName>() {
            cors = cors.allowed_header(name);
        }
    }

    let expose: Vec<actix_web::http::header::HeaderName> =
        config.expose_headers.iter().filter_map(|h| h.parse().ok()).collect();
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
        let _ = build_cors(&CorsConfig::restrictive(["https://a.example.com"]));
    }

    #[test]
    fn config_serde_round_trips() {
        let config = CorsConfig::restrictive(["https://a.example.com"]);
        let json = serde_json::to_string(&config).unwrap();
        let back: CorsConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.allowed_origins, config.allowed_origins);
        assert_eq!(back.allow_credentials, config.allow_credentials);
    }
}
