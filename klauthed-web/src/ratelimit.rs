//! In-memory fixed-window rate limiting middleware.
//!
//! [`RateLimit`] is an actix [`Transform`] that caps how many requests a given
//! client may make within a rolling fixed window. Clients are keyed by a
//! configurable strategy ([`KeyBy`]) — peer IP by default, or the value of a
//! header such as `x-api-key`. When a client exceeds its budget the request is
//! rejected with `429 Too Many Requests` (via [`AppError`], category
//! `RateLimited`) and a `Retry-After` header indicating when the window resets.
//!
//! State is held in a `Mutex<HashMap>` shared across workers; counters reset
//! lazily when a window elapses, so memory is bounded by the number of distinct
//! active keys.
//!
//! ```no_run
//! use std::time::Duration;
//! use actix_web::App;
//! use klauthed_web::ratelimit::{KeyBy, RateLimit};
//!
//! // 100 requests per minute, keyed by the `x-api-key` header.
//! let limiter = RateLimit::new(100, Duration::from_secs(60))
//!     .key_by(KeyBy::header("x-api-key"));
//!
//! let app = App::new().wrap(limiter);
//! ```
//!
//! # Out of scope (future passes)
//!
//! Distributed limiting (shared store), token-bucket smoothing, and per-route
//! budgets are intentionally not handled here yet.

use std::collections::HashMap;
use std::future::{ready, Ready};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::task::{Context as TaskContext, Poll};
use std::time::{Duration, Instant};

use actix_web::body::EitherBody;
use actix_web::dev::{Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::http::header::{HeaderName, HeaderValue, RETRY_AFTER};
use actix_web::{Error, HttpMessage as _, ResponseError};
use futures_util::future::LocalBoxFuture;
use klauthed_security::Claims;

use crate::error::AppError;

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
    fn key_for(&self, req: &ServiceRequest) -> String {
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

/// One client's counter within the current window.
#[derive(Debug, Clone, Copy)]
struct Window {
    /// When the current window started.
    started: Instant,
    /// Requests seen in the current window.
    count: u32,
}

/// Shared limiter state: a counter per client key.
#[derive(Debug, Default)]
struct State {
    windows: Mutex<HashMap<String, Window>>,
}

/// Outcome of recording one request against a key.
enum Decision {
    /// Allowed; nothing more to do.
    Allowed,
    /// Rejected; retry after the given duration.
    Limited { retry_after: Duration },
}

impl State {
    /// Record a request for `key`, returning whether it is allowed.
    fn check(&self, key: &str, max: u32, window: Duration, now: Instant) -> Decision {
        let mut windows = self.windows.lock().expect("rate-limit mutex poisoned");
        let entry = windows.entry(key.to_owned()).or_insert(Window {
            started: now,
            count: 0,
        });

        // Reset the window if it has elapsed.
        if now.duration_since(entry.started) >= window {
            entry.started = now;
            entry.count = 0;
        }

        if entry.count >= max {
            let elapsed = now.duration_since(entry.started);
            let retry_after = window.saturating_sub(elapsed);
            Decision::Limited { retry_after }
        } else {
            entry.count += 1;
            Decision::Allowed
        }
    }
}

/// Fixed-window rate limiter middleware (an actix [`Transform`]).
///
/// Construct with [`RateLimit::new`] (max requests + window), optionally choose
/// a [`KeyBy`] strategy, and `.wrap(...)` it on an `App`/scope.
#[derive(Debug, Clone)]
pub struct RateLimit {
    max_requests: u32,
    window: Duration,
    key_by: KeyBy,
    state: Arc<State>,
}

impl RateLimit {
    /// A limiter allowing `max_requests` per `window`, keyed by peer IP.
    ///
    /// `max_requests` is clamped to at least 1.
    pub fn new(max_requests: u32, window: Duration) -> Self {
        Self {
            max_requests: max_requests.max(1),
            window,
            key_by: KeyBy::PeerIp,
            state: Arc::new(State::default()),
        }
    }

    /// Choose how requests are mapped to buckets (builder form).
    pub fn key_by(mut self, key_by: KeyBy) -> Self {
        self.key_by = key_by;
        self
    }

    /// The configured request ceiling per window.
    pub fn max_requests(&self) -> u32 {
        self.max_requests
    }

    /// The configured window length.
    pub fn window(&self) -> Duration {
        self.window
    }
}

impl<S, B> Transform<S, ServiceRequest> for RateLimit
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Transform = RateLimitService<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(RateLimitService {
            service: Rc::new(service),
            max_requests: self.max_requests,
            window: self.window,
            key_by: self.key_by.clone(),
            state: Arc::clone(&self.state),
        }))
    }
}

/// The [`Service`] produced by [`RateLimit`].
pub struct RateLimitService<S> {
    service: Rc<S>,
    max_requests: u32,
    window: Duration,
    key_by: KeyBy,
    state: Arc<State>,
}

impl<S, B> Service<ServiceRequest> for RateLimitService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&self, cx: &mut TaskContext<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let key = self.key_by.key_for(&req);
        let decision = self
            .state
            .check(&key, self.max_requests, self.window, Instant::now());

        match decision {
            Decision::Allowed => {
                let fut = self.service.call(req);
                Box::pin(async move { fut.await.map(ServiceResponse::map_into_left_body) })
            }
            Decision::Limited { retry_after } => {
                let secs = retry_after.as_secs().max(1);
                Box::pin(async move {
                    let err = AppError::too_many_requests(format!(
                        "rate limit exceeded; retry after {secs}s"
                    ));
                    let mut resp = err.error_response();
                    if let Ok(value) = HeaderValue::from_str(&secs.to_string()) {
                        resp.headers_mut().insert(RETRY_AFTER, value);
                    }
                    // Re-attach the original request to form a ServiceResponse.
                    Ok(req.into_response(resp).map_into_right_body())
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::http::StatusCode;
    use actix_web::{test, web, App, HttpResponse};

    #[std::prelude::v1::test]
    fn fixed_window_allows_then_limits_then_resets() {
        let state = State::default();
        let window = Duration::from_secs(10);
        let t0 = Instant::now();

        // First two allowed, third limited.
        assert!(matches!(state.check("k", 2, window, t0), Decision::Allowed));
        assert!(matches!(state.check("k", 2, window, t0), Decision::Allowed));
        assert!(matches!(
            state.check("k", 2, window, t0),
            Decision::Limited { .. }
        ));

        // After the window elapses, the budget refreshes.
        let t1 = t0 + window;
        assert!(matches!(state.check("k", 2, window, t1), Decision::Allowed));
    }

    #[std::prelude::v1::test]
    fn keys_are_independent() {
        let state = State::default();
        let window = Duration::from_secs(10);
        let now = Instant::now();
        assert!(matches!(state.check("a", 1, window, now), Decision::Allowed));
        assert!(matches!(
            state.check("a", 1, window, now),
            Decision::Limited { .. }
        ));
        // A different key has its own fresh budget.
        assert!(matches!(state.check("b", 1, window, now), Decision::Allowed));
    }

    async fn ok() -> HttpResponse {
        HttpResponse::Ok().finish()
    }

    #[actix_web::test]
    async fn middleware_allows_n_then_429_with_retry_after() {
        let limiter = RateLimit::new(2, Duration::from_secs(60))
            .key_by(KeyBy::header("x-api-key"));
        let app = test::init_service(
            App::new().wrap(limiter).route("/", web::get().to(ok)),
        )
        .await;

        let make = || {
            test::TestRequest::get()
                .uri("/")
                .insert_header(("x-api-key", "client-1"))
                .to_request()
        };

        assert_eq!(test::call_service(&app, make()).await.status(), StatusCode::OK);
        assert_eq!(test::call_service(&app, make()).await.status(), StatusCode::OK);

        let resp = test::call_service(&app, make()).await;
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
        let retry = resp
            .headers()
            .get(RETRY_AFTER)
            .expect("Retry-After header present")
            .to_str()
            .unwrap();
        assert!(retry.parse::<u64>().unwrap() >= 1);
    }

    #[actix_web::test]
    async fn distinct_clients_have_separate_budgets() {
        let limiter = RateLimit::new(1, Duration::from_secs(60))
            .key_by(KeyBy::header("x-api-key"));
        let app = test::init_service(
            App::new().wrap(limiter).route("/", web::get().to(ok)),
        )
        .await;

        let req_a = test::TestRequest::get()
            .uri("/")
            .insert_header(("x-api-key", "a"))
            .to_request();
        let req_b = test::TestRequest::get()
            .uri("/")
            .insert_header(("x-api-key", "b"))
            .to_request();

        assert_eq!(test::call_service(&app, req_a).await.status(), StatusCode::OK);
        assert_eq!(test::call_service(&app, req_b).await.status(), StatusCode::OK);
    }

    #[actix_web::test]
    async fn principal_key_uses_jwt_sub_when_present() {
        use klauthed_security::{jwt::JwtSigner, JwtVerifier};
        use crate::auth::JwtAuth;

        const SECRET: &[u8] = b"ratelimit-test-secret";

        // Mint a token for "alice".
        let token = JwtSigner::hs256(SECRET)
            .encode(
                &klauthed_security::Claims::builder(
                    "alice",
                    &klauthed_core::time::SystemClock,
                    klauthed_core::time::Duration::hours(1),
                )
                .build(),
            )
            .unwrap();

        // 1 request allowed per user; alice and bob have independent budgets.
        let limiter = RateLimit::new(1, Duration::from_secs(60))
            .key_by(KeyBy::Principal);
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(JwtVerifier::hs256(SECRET)))
                .wrap(limiter)
                .wrap(JwtAuth::new())
                .route("/", web::get().to(ok)),
        )
        .await;

        // First request as alice: allowed.
        let req1 = test::TestRequest::get()
            .uri("/")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        assert_eq!(test::call_service(&app, req1).await.status(), StatusCode::OK);

        // Second request as alice: rate-limited.
        let req2 = test::TestRequest::get()
            .uri("/")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        assert_eq!(
            test::call_service(&app, req2).await.status(),
            StatusCode::TOO_MANY_REQUESTS
        );
    }
}
