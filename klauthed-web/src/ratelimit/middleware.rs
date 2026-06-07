//! The [`RateLimit`] actix [`Transform`] and its [`RateLimitService`].

use std::fmt;
use std::future::{Ready, ready};
use std::rc::Rc;
use std::sync::Arc;
use std::task::{Context as TaskContext, Poll};
use std::time::Duration;

use actix_web::body::EitherBody;
use actix_web::dev::{Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::http::header::{HeaderValue, RETRY_AFTER};
use actix_web::{Error, ResponseError};
use futures_util::future::LocalBoxFuture;
use klauthed_data::rate_limit::{InMemoryRateLimiter, RateLimitOutcome, RateLimiter};

use super::key::KeyBy;
use crate::error::AppError;

/// Fixed-window rate limiter middleware (an actix [`Transform`]).
///
/// Construct with [`RateLimit::new`] (max requests + window) for the default
/// per-process limiter, or [`RateLimit::with_store`] to share one budget across
/// replicas (e.g. a `RedisRateLimiter`). Optionally choose a [`KeyBy`] strategy,
/// then `.wrap(...)` it on an `App`/scope.
#[derive(Clone)]
pub struct RateLimit {
    max_requests: u32,
    window: Duration,
    key_by: KeyBy,
    limiter: Arc<dyn RateLimiter>,
}

impl fmt::Debug for RateLimit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RateLimit")
            .field("max_requests", &self.max_requests)
            .field("window", &self.window)
            .field("key_by", &self.key_by)
            .finish_non_exhaustive()
    }
}

impl RateLimit {
    /// A limiter allowing `max_requests` per `window`, keyed by peer IP, backed
    /// by an in-process [`InMemoryRateLimiter`] (each replica counts on its own).
    ///
    /// `max_requests` is clamped to at least 1.
    #[must_use]
    pub fn new(max_requests: u32, window: Duration) -> Self {
        Self::with_store(Arc::new(InMemoryRateLimiter::system()), max_requests, window)
    }

    /// A limiter backed by a shared [`RateLimiter`] store (e.g. Redis), so a
    /// fleet of replicas enforces one global budget per key.
    #[must_use]
    pub fn with_store(limiter: Arc<dyn RateLimiter>, max_requests: u32, window: Duration) -> Self {
        Self { max_requests: max_requests.max(1), window, key_by: KeyBy::PeerIp, limiter }
    }

    /// Choose how requests are mapped to buckets (builder form).
    #[must_use]
    pub fn key_by(mut self, key_by: KeyBy) -> Self {
        self.key_by = key_by;
        self
    }

    /// The configured request ceiling per window.
    #[must_use]
    pub fn max_requests(&self) -> u32 {
        self.max_requests
    }

    /// The configured window length.
    #[must_use]
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
            limiter: Arc::clone(&self.limiter),
        }))
    }
}

/// The [`Service`] produced by [`RateLimit`].
pub struct RateLimitService<S> {
    service: Rc<S>,
    max_requests: u32,
    window: Duration,
    key_by: KeyBy,
    limiter: Arc<dyn RateLimiter>,
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
        let limiter = Arc::clone(&self.limiter);
        let max = self.max_requests;
        let window = self.window;
        let service = Rc::clone(&self.service);

        Box::pin(async move {
            match limiter.check(&key, max, window).await {
                Ok(RateLimitOutcome::Allowed { .. }) => {
                    service.call(req).await.map(ServiceResponse::map_into_left_body)
                }
                Ok(RateLimitOutcome::Limited { retry_after }) => {
                    let secs = retry_after.as_secs().max(1);
                    let err = AppError::too_many_requests(format!(
                        "rate limit exceeded; retry after {secs}s"
                    ));
                    let mut resp = err.error_response();
                    if let Ok(value) = HeaderValue::from_str(&secs.to_string()) {
                        resp.headers_mut().insert(RETRY_AFTER, value);
                    }
                    Ok(req.into_response(resp).map_into_right_body())
                }
                Err(error) => {
                    // Fail open: a limiter-backend outage (e.g. Redis down) must
                    // not take the service down. Log and let the request through.
                    tracing::warn!(%error, "rate-limit backend error; allowing request");
                    service.call(req).await.map(ServiceResponse::map_into_left_body)
                }
            }
        })
    }
}
