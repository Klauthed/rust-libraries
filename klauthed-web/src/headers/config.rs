//! Configuration for the [`SecurityHeaders`](super::SecurityHeaders) middleware.

use actix_web::http::header::{HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};

/// HTTP Strict-Transport-Security policy (`Strict-Transport-Security`).
///
/// Tells browsers to only ever contact the origin over HTTPS for `max_age_secs`
/// seconds. Browsers ignore the header on plain-HTTP responses, so it is safe to
/// emit unconditionally; disable it in local development with
/// [`SecurityHeadersConfig::without_hsts`] if you serve over `http://`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hsts {
    /// `max-age` in seconds — how long browsers should force HTTPS.
    pub max_age_secs: u64,
    /// Append `includeSubDomains` so the policy also covers every subdomain.
    pub include_subdomains: bool,
    /// Append `preload`. Only set this once the domain is submitted to the
    /// browser preload list; it implies a long `max_age` and `includeSubDomains`.
    pub preload: bool,
}

impl Hsts {
    /// The recommended policy: two years, including subdomains, without `preload`.
    #[must_use]
    pub fn recommended() -> Self {
        Self { max_age_secs: 63_072_000, include_subdomains: true, preload: false }
    }

    /// Render the header value, e.g. `max-age=63072000; includeSubDomains`.
    fn header_value(&self) -> String {
        let mut value = format!("max-age={}", self.max_age_secs);
        if self.include_subdomains {
            value.push_str("; includeSubDomains");
        }
        if self.preload {
            value.push_str("; preload");
        }
        value
    }
}

impl Default for Hsts {
    fn default() -> Self {
        Self::recommended()
    }
}

/// `X-Frame-Options` policy — controls whether the response may be framed,
/// defending against clickjacking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FrameOptions {
    /// `DENY` — never allow framing (the default; right for auth endpoints).
    #[default]
    Deny,
    /// `SAMEORIGIN` — allow framing only by same-origin pages.
    SameOrigin,
}

impl FrameOptions {
    /// The literal header value.
    fn header_value(self) -> &'static str {
        match self {
            FrameOptions::Deny => "DENY",
            FrameOptions::SameOrigin => "SAMEORIGIN",
        }
    }
}

/// Which security response headers to emit, and with what values.
///
/// The [`default`](Default::default) is tuned for JSON / auth APIs: strict
/// everywhere — deny framing, no referrer, HSTS on, `nosniff`, a locked-down CSP
/// (`default-src 'none'`), and same-origin cross-origin isolation. Relax
/// individual headers with the builder methods, or start from
/// [`relaxed`](Self::relaxed) when the same app also serves HTML pages.
///
/// Implements [`serde::Deserialize`] so it can live in a service config file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SecurityHeadersConfig {
    /// `Strict-Transport-Security`. `None` omits the header.
    pub hsts: Option<Hsts>,
    /// `X-Frame-Options`. `None` omits the header.
    pub frame_options: Option<FrameOptions>,
    /// Emit `X-Content-Type-Options: nosniff`.
    pub content_type_nosniff: bool,
    /// `Referrer-Policy` value. `None` omits the header.
    pub referrer_policy: Option<String>,
    /// `Content-Security-Policy` value. `None` omits the header.
    pub content_security_policy: Option<String>,
    /// `Permissions-Policy` value. `None` omits the header.
    pub permissions_policy: Option<String>,
    /// `Cross-Origin-Opener-Policy` value. `None` omits the header.
    pub cross_origin_opener_policy: Option<String>,
    /// `Cross-Origin-Resource-Policy` value. `None` omits the header.
    pub cross_origin_resource_policy: Option<String>,
}

impl Default for SecurityHeadersConfig {
    fn default() -> Self {
        Self {
            hsts: Some(Hsts::recommended()),
            frame_options: Some(FrameOptions::Deny),
            content_type_nosniff: true,
            referrer_policy: Some("no-referrer".to_owned()),
            // Appropriate for a JSON API: the response is not a document, so deny
            // every resource type. HTML-serving scopes should override this.
            content_security_policy: Some(
                "default-src 'none'; frame-ancestors 'none'; base-uri 'none'".to_owned(),
            ),
            permissions_policy: None,
            cross_origin_opener_policy: Some("same-origin".to_owned()),
            cross_origin_resource_policy: Some("same-origin".to_owned()),
        }
    }
}

impl SecurityHeadersConfig {
    /// Defaults suited to an app that also serves HTML pages: framing allowed
    /// from the same origin, a document-friendly CSP (`default-src 'self'`), and
    /// the cross-origin isolation headers left off.
    #[must_use]
    pub fn relaxed() -> Self {
        Self {
            frame_options: Some(FrameOptions::SameOrigin),
            content_security_policy: Some(
                "default-src 'self'; object-src 'none'; frame-ancestors 'self'; base-uri 'self'"
                    .to_owned(),
            ),
            cross_origin_opener_policy: None,
            cross_origin_resource_policy: None,
            ..Self::default()
        }
    }

    /// Drop the `Strict-Transport-Security` header (e.g. for local HTTP dev).
    #[must_use]
    pub fn without_hsts(mut self) -> Self {
        self.hsts = None;
        self
    }

    /// Set the `Strict-Transport-Security` policy.
    #[must_use]
    pub fn with_hsts(mut self, hsts: Hsts) -> Self {
        self.hsts = Some(hsts);
        self
    }

    /// Set the `Content-Security-Policy` value.
    #[must_use]
    pub fn with_csp(mut self, policy: impl Into<String>) -> Self {
        self.content_security_policy = Some(policy.into());
        self
    }

    /// Drop the `Content-Security-Policy` header.
    #[must_use]
    pub fn without_csp(mut self) -> Self {
        self.content_security_policy = None;
        self
    }

    /// Set the `X-Frame-Options` policy.
    #[must_use]
    pub fn with_frame_options(mut self, frame_options: FrameOptions) -> Self {
        self.frame_options = Some(frame_options);
        self
    }

    /// Set the `Referrer-Policy` value.
    #[must_use]
    pub fn with_referrer_policy(mut self, policy: impl Into<String>) -> Self {
        self.referrer_policy = Some(policy.into());
        self
    }

    /// Set the `Permissions-Policy` value.
    #[must_use]
    pub fn with_permissions_policy(mut self, policy: impl Into<String>) -> Self {
        self.permissions_policy = Some(policy.into());
        self
    }

    /// Pre-render the configured headers into `(name, value)` pairs. Entries with
    /// values that are not valid header bytes are skipped.
    pub(crate) fn header_pairs(&self) -> Vec<(HeaderName, HeaderValue)> {
        let mut pairs: Vec<(HeaderName, HeaderValue)> = Vec::new();

        let mut push = |name: &'static str, value: &str| {
            if let Ok(value) = HeaderValue::from_str(value) {
                pairs.push((HeaderName::from_static(name), value));
            }
        };

        if let Some(hsts) = &self.hsts {
            push("strict-transport-security", &hsts.header_value());
        }
        if let Some(frame) = self.frame_options {
            push("x-frame-options", frame.header_value());
        }
        if self.content_type_nosniff {
            push("x-content-type-options", "nosniff");
        }
        if let Some(policy) = &self.referrer_policy {
            push("referrer-policy", policy);
        }
        if let Some(policy) = &self.content_security_policy {
            push("content-security-policy", policy);
        }
        if let Some(policy) = &self.permissions_policy {
            push("permissions-policy", policy);
        }
        if let Some(policy) = &self.cross_origin_opener_policy {
            push("cross-origin-opener-policy", policy);
        }
        if let Some(policy) = &self.cross_origin_resource_policy {
            push("cross-origin-resource-policy", policy);
        }

        pairs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsts_renders_directives() {
        assert_eq!(Hsts::recommended().header_value(), "max-age=63072000; includeSubDomains");
        let full = Hsts { max_age_secs: 100, include_subdomains: true, preload: true };
        assert_eq!(full.header_value(), "max-age=100; includeSubDomains; preload");
        let bare = Hsts { max_age_secs: 100, include_subdomains: false, preload: false };
        assert_eq!(bare.header_value(), "max-age=100");
    }

    #[test]
    fn default_emits_the_strict_set() {
        let pairs = SecurityHeadersConfig::default().header_pairs();
        let names: Vec<_> = pairs.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"strict-transport-security"));
        assert!(names.contains(&"x-frame-options"));
        assert!(names.contains(&"x-content-type-options"));
        assert!(names.contains(&"content-security-policy"));
        assert!(names.contains(&"cross-origin-opener-policy"));
    }

    #[test]
    fn without_hsts_drops_the_header() {
        let pairs = SecurityHeadersConfig::default().without_hsts().header_pairs();
        assert!(!pairs.iter().any(|(n, _)| n.as_str() == "strict-transport-security"));
    }

    #[test]
    fn frame_options_values() {
        assert_eq!(FrameOptions::Deny.header_value(), "DENY");
        assert_eq!(FrameOptions::SameOrigin.header_value(), "SAMEORIGIN");
    }

    #[test]
    fn relaxed_allows_same_origin_framing_and_drops_isolation() {
        let cfg = SecurityHeadersConfig::relaxed();
        assert_eq!(cfg.frame_options, Some(FrameOptions::SameOrigin));
        assert!(cfg.cross_origin_opener_policy.is_none());
    }

    #[test]
    fn config_serde_round_trips() {
        let cfg = SecurityHeadersConfig::default().with_csp("default-src 'self'");
        let json = serde_json::to_string(&cfg).unwrap();
        let back: SecurityHeadersConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back, cfg);
    }
}
