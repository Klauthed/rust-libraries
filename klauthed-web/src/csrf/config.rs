//! Configuration for the [`Csrf`](super::Csrf) middleware.

use actix_web::cookie::SameSite;
use serde::{Deserialize, Serialize};

/// `SameSite` attribute for the CSRF cookie.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CsrfSameSite {
    /// `SameSite=Lax` (the default) — sent on top-level navigations but not on
    /// cross-site subrequests. A good baseline that also limits CSRF on its own.
    #[default]
    Lax,
    /// `SameSite=Strict` — never sent on any cross-site request.
    Strict,
    /// `SameSite=None` — sent on all cross-site requests (requires `Secure`).
    None,
}

impl From<CsrfSameSite> for SameSite {
    fn from(value: CsrfSameSite) -> Self {
        match value {
            CsrfSameSite::Lax => SameSite::Lax,
            CsrfSameSite::Strict => SameSite::Strict,
            CsrfSameSite::None => SameSite::None,
        }
    }
}

/// Settings for the [`Csrf`](super::Csrf) double-submit-cookie middleware.
///
/// The defaults implement the standard double-submit pattern: a random token is
/// stored in a JavaScript-readable cookie and must be echoed back in a request
/// header on every unsafe (state-changing) request. Implements
/// [`serde::Deserialize`] so it can live in a service config file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CsrfConfig {
    /// Name of the cookie holding the CSRF token (default `csrf_token`).
    pub cookie_name: String,
    /// Request header that must echo the cookie value (default `x-csrf-token`).
    pub header_name: String,
    /// `Path` attribute for the issued cookie (default `/`).
    pub cookie_path: String,
    /// `SameSite` attribute for the issued cookie (default `Lax`).
    pub same_site: CsrfSameSite,
    /// Set the `Secure` attribute so the cookie is only sent over HTTPS
    /// (default `true`; turn off for local `http://` development).
    pub secure: bool,
    /// On a safe request without a CSRF cookie, mint one and set it on the
    /// response so the client can start echoing it (default `true`).
    pub auto_issue: bool,
    /// Skip the check for requests carrying an `Authorization: Bearer` token.
    /// Bearer-authenticated APIs are not exposed to CSRF (no ambient cookie
    /// credentials), so this is safe and on by default.
    pub skip_bearer: bool,
    /// Number of random bytes in a freshly minted token (default `32`).
    pub token_bytes: usize,
}

impl Default for CsrfConfig {
    fn default() -> Self {
        Self {
            cookie_name: "csrf_token".to_owned(),
            header_name: "x-csrf-token".to_owned(),
            cookie_path: "/".to_owned(),
            same_site: CsrfSameSite::Lax,
            secure: true,
            auto_issue: true,
            skip_bearer: true,
            token_bytes: 32,
        }
    }
}

impl CsrfConfig {
    /// Set the cookie `Secure` attribute (turn off for local HTTP dev).
    #[must_use]
    pub fn secure(mut self, secure: bool) -> Self {
        self.secure = secure;
        self
    }

    /// Set the cookie name.
    #[must_use]
    pub fn cookie_name(mut self, name: impl Into<String>) -> Self {
        self.cookie_name = name.into();
        self
    }

    /// Set the request header that must echo the cookie value.
    #[must_use]
    pub fn header_name(mut self, name: impl Into<String>) -> Self {
        self.header_name = name.into();
        self
    }

    /// Set the cookie `SameSite` attribute.
    #[must_use]
    pub fn same_site(mut self, same_site: CsrfSameSite) -> Self {
        self.same_site = same_site;
        self
    }

    /// Enable or disable auto-issuing a cookie on safe requests.
    #[must_use]
    pub fn auto_issue(mut self, auto_issue: bool) -> Self {
        self.auto_issue = auto_issue;
        self
    }

    /// Enable or disable skipping the check for `Bearer`-authenticated requests.
    #[must_use]
    pub fn skip_bearer(mut self, skip_bearer: bool) -> Self {
        self.skip_bearer = skip_bearer;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_double_submit() {
        let c = CsrfConfig::default();
        assert_eq!(c.cookie_name, "csrf_token");
        assert_eq!(c.header_name, "x-csrf-token");
        assert!(c.secure);
        assert!(c.auto_issue);
        assert!(c.skip_bearer);
        assert_eq!(c.token_bytes, 32);
    }

    #[test]
    fn same_site_maps_to_actix() {
        assert_eq!(SameSite::from(CsrfSameSite::Lax), SameSite::Lax);
        assert_eq!(SameSite::from(CsrfSameSite::Strict), SameSite::Strict);
        assert_eq!(SameSite::from(CsrfSameSite::None), SameSite::None);
    }

    #[test]
    fn builder_overrides() {
        let c = CsrfConfig::default().secure(false).cookie_name("xsrf").skip_bearer(false);
        assert!(!c.secure);
        assert_eq!(c.cookie_name, "xsrf");
        assert!(!c.skip_bearer);
    }

    #[test]
    fn config_serde_round_trips() {
        let c = CsrfConfig::default().same_site(CsrfSameSite::Strict);
        let json = serde_json::to_string(&c).unwrap();
        let back: CsrfConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back, c);
    }
}
