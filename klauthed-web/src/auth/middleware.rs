//! JWT Bearer authentication middleware ([`JwtAuth`]) plus the optional
//! [`TokenRevocationCheck`] denylist hook.

use std::future::{ready, Ready};
use std::rc::Rc;
use std::sync::Arc;
use std::task::{Context as TaskContext, Poll};

use actix_web::body::BoxBody;
use actix_web::dev::{Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::http::header::AUTHORIZATION;
use actix_web::{web, Error, HttpMessage as _, ResponseError as _};
use futures_util::future::LocalBoxFuture;
use klauthed_core::context::RequestContext;
use klauthed_error::DomainError as _;
use klauthed_security::{JwtVerifier, SecurityError, TokenDenylist};

use crate::error::AppError;

// ── TokenRevocationCheck ──────────────────────────────────────────────────────

/// A concrete wrapper around `Arc<dyn TokenDenylist>` for actix app data.
///
/// Register this with `App::app_data(web::Data::new(TokenRevocationCheck(denylist)))`.
/// When present, [`JwtAuth`] will check every token's `jti` against the
/// denylist before admitting the request.
///
/// ```no_run
/// use std::sync::Arc;
/// use actix_web::{web, App};
/// use klauthed_security::InMemoryTokenDenylist;
/// use klauthed_web::auth::{JwtAuth, TokenRevocationCheck};
///
/// let denylist = Arc::new(InMemoryTokenDenylist::new());
/// let _app = App::new()
///     .app_data(web::Data::new(TokenRevocationCheck(denylist)))
///     .wrap(JwtAuth::new());
/// ```
pub struct TokenRevocationCheck(pub Arc<dyn TokenDenylist>);

/// `"Bearer "` — the mandatory prefix for the `Authorization` header value.
const BEARER_PREFIX: &str = "Bearer ";

/// Extract `<token>` from an `Authorization: Bearer <token>` header on a
/// `ServiceRequest`, or `None` when the header is absent or uses a different
/// scheme.
fn extract_bearer_token(req: &ServiceRequest) -> Option<String> {
    let value = req.headers().get(AUTHORIZATION)?;
    let s = value.to_str().ok()?;
    s.strip_prefix(BEARER_PREFIX).map(str::to_owned)
}

// ── JwtAuth middleware ────────────────────────────────────────────────────────

/// Actix [`Transform`] that validates JWT Bearer tokens.
///
/// Wrap a scope or the whole app with `.wrap(JwtAuth::new())` and register the
/// configured [`JwtVerifier`] as `web::Data<JwtVerifier>`. Every request that
/// passes through will have its `Authorization: Bearer` token decoded and the
/// resulting [`Claims`](klauthed_security::Claims) stored in the request extensions.
///
/// Requests without a Bearer token, or with an invalid / expired token, are
/// rejected with `401 Unauthorized` before any handler runs.
///
/// # Panics
///
/// Does not panic; a missing `web::Data<JwtVerifier>` returns `500` with a
/// logged error so misconfiguration surfaces clearly in development.
#[derive(Debug, Clone, Default)]
pub struct JwtAuth {
    _priv: (),
}

impl JwtAuth {
    /// Construct the middleware.
    pub fn new() -> Self {
        Self::default()
    }
}

impl<S, B> Transform<S, ServiceRequest> for JwtAuth
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: actix_web::body::MessageBody + 'static,
{
    type Response = ServiceResponse<BoxBody>;
    type Error = Error;
    type Transform = JwtAuthService<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(JwtAuthService {
            service: Rc::new(service),
        }))
    }
}

/// The [`Service`] produced by [`JwtAuth`].
pub struct JwtAuthService<S> {
    service: Rc<S>,
}

/// Reject a request with `err`, boxing the error response body.
fn reject(req: ServiceRequest, err: AppError) -> ServiceResponse<BoxBody> {
    req.into_response(err.error_response().map_into_boxed_body())
}

impl<S, B> Service<ServiceRequest> for JwtAuthService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: actix_web::body::MessageBody + 'static,
{
    type Response = ServiceResponse<BoxBody>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&self, cx: &mut TaskContext<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&self, req: ServiceRequest) -> Self::Future {
        // A missing JwtVerifier is a server configuration error, not a caller
        // mistake — log it loudly and surface 500 without leaking details.
        let Some(verifier) = req.app_data::<web::Data<JwtVerifier>>().cloned() else {
            tracing::error!(
                "JwtAuth: no JwtVerifier registered as web::Data — \
                 did you forget App::app_data(web::Data::new(verifier))?"
            );
            return Box::pin(ready(Ok(reject(
                req,
                AppError::internal("authentication is not configured"),
            ))));
        };

        // A missing Authorization header or wrong scheme → 401 immediately.
        let Some(token) = extract_bearer_token(&req) else {
            return Box::pin(ready(Ok(reject(
                req,
                AppError::unauthorized("authentication required"),
            ))));
        };

        // Validate the token; map SecurityError to a client-safe 401/400.
        let claims = match verifier.decode(&token) {
            Ok(c) => c,
            Err(e) => {
                // Log the technical reason server-side; send a minimal message to
                // the client so validation internals are never exposed.
                let client_msg = match &e {
                    SecurityError::ExpiredToken => "token has expired",
                    SecurityError::MalformedToken(_) => "token is malformed",
                    _ => "token is invalid",
                };
                tracing::debug!(error = %e, "jwt bearer validation failed");
                let app_err = AppError::new(e.category(), client_msg).with_code(e.code());
                return Box::pin(ready(Ok(reject(req, app_err))));
            }
        };

        // Happy path: propagate the authenticated subject into RequestContext so
        // audit events, observability spans, and structured logs all carry the
        // principal without the handler having to thread it through manually.
        if let Some(sub) = claims.sub.as_deref() {
            let updated_ctx = req
                .extensions()
                .get::<RequestContext>()
                .cloned()
                .unwrap_or_default()
                .with_principal(sub);
            req.extensions_mut().insert(updated_ctx);
        }

        // Extract jti before moving claims into extensions (needed for revocation
        // check inside the async block).
        let jti = claims.jti.clone();

        // Store decoded claims for the AuthenticatedUser / OptionalAuthentication
        // extractors — separate from RequestContext so both are independently
        // accessible.
        req.extensions_mut().insert(claims);

        // Optional: check jti against a registered TokenDenylist.
        // If web::Data<TokenRevocationCheck> is present AND the token carries
        // a jti, we check whether it has been revoked.
        let denylist = req
            .app_data::<web::Data<TokenRevocationCheck>>()
            .map(|d| Arc::clone(&d.0));

        let service = Rc::clone(&self.service);
        Box::pin(async move {
            if let (Some(ref dl), Some(ref jti)) = (denylist, jti) {
                match dl.is_revoked(jti).await {
                    Ok(true) => {
                        return Ok(reject(req, AppError::unauthorized("token has been revoked")));
                    }
                    Ok(false) => {}
                    Err(e) => {
                        tracing::error!(error = %e, "token denylist check failed");
                        return Ok(reject(
                            req,
                            AppError::internal("authentication check failed"),
                        ));
                    }
                }
            }
            service
                .call(req)
                .await
                .map(ServiceResponse::map_into_boxed_body)
        })
    }
}
