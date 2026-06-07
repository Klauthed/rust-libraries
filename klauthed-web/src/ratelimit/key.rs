//! The [`KeyBy`] bucket-keying strategy and peer-IP resolution.

use actix_web::HttpMessage as _;
use actix_web::dev::ServiceRequest;
use actix_web::http::header::HeaderName;
use klauthed_security::Claims;

/// How a request is mapped to a rate-limit bucket key.
///
/// Choose the strategy that best models the threat you're protecting against:
/// `PeerIp` for anonymous traffic, `Principal` for authenticated abuse,
/// `OAuthClient` for per-client API quotas.
#[derive(Debug, Clone)]
pub enum KeyBy {
    /// Key by the connection peer IP address. Requests without a resolvable
    /// peer address share the `"unknown"` bucket.
    PeerIp,
    /// Key by the value of the named request header. Requests missing the
    /// header share the `"anonymous"` bucket.
    Header(HeaderName),
    /// Key by the authenticated user's `sub` claim (JWT).
    ///
    /// Requires [`JwtAuth`](crate::auth::JwtAuth) to run first so the claims
    /// are in the request extensions. Falls back to peer IP for unauthenticated
    /// requests.
    Principal,
    /// Key by the `client_id` claim embedded in the JWT by the token endpoint.
    ///
    /// Useful for per-OAuth-client API quotas. Falls back to peer IP when the
    /// claim is absent.
    OAuthClient,
}

impl KeyBy {
    /// Key by the given header name (case-insensitive).
    ///
    /// # Panics
    ///
    /// Panics if `name` is not a valid HTTP header name.
    pub fn header(name: &str) -> Self {
        KeyBy::Header(HeaderName::from_bytes(name.as_bytes()).expect("valid header name"))
    }

    /// Resolve the bucket key for a request.
    pub(crate) fn key_for(&self, req: &ServiceRequest) -> String {
        match self {
            KeyBy::PeerIp => peer_ip(req),
            KeyBy::Header(name) => req
                .headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_owned())
                .unwrap_or_else(|| "anonymous".to_owned()),
            KeyBy::Principal => req
                .extensions()
                .get::<Claims>()
                .and_then(|c| c.sub.clone())
                .unwrap_or_else(|| peer_ip(req)),
            KeyBy::OAuthClient => req
                .extensions()
                .get::<Claims>()
                .and_then(|c| c.custom.get("client_id"))
                .and_then(|v| v.as_str())
                .map(str::to_owned)
                .unwrap_or_else(|| peer_ip(req)),
        }
    }
}

/// Extract the real peer IP, falling back to `"unknown"`.
fn peer_ip(req: &ServiceRequest) -> String {
    req.connection_info()
        .realip_remote_addr()
        .map(str::to_owned)
        .unwrap_or_else(|| "unknown".to_owned())
}
