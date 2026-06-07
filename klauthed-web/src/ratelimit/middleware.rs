//! The [`RateLimit`] actix [`Transform`] and its [`RateLimitService`].

use std::future::{Ready, ready};
use std::rc::Rc;
use std::sync::Arc;
use std::task::{Context as TaskContext, Poll};
use std::time::{Duration, Instant};

use actix_web::body::EitherBody;
use actix_web::dev::{Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::http::header::{HeaderValue, RETRY_AFTER};
use actix_web::{Error, ResponseError};
use futures_util::future::LocalBoxFuture;

use super::key::KeyBy;
use super::state::{Decision, State};
use crate::error::AppError;

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
        let decision = self.state.check(&key, self.max_requests, self.window, Instant::now());

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
