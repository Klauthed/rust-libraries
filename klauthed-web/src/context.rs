//! Per-request [`RequestContext`] plumbing for actix-web.
//!
//! This module provides two pieces that work together:
//!
//! * [`RequestContextMiddleware`] — an actix [`Transform`] that, for every
//!   request, builds a [`RequestContext`] from inbound headers
//!   (`x-request-id`, `x-correlation-id`, `x-tenant-id`, `Accept-Language`),
//!   stores it in the request extensions, and echoes the resolved request id
//!   back on the response as `x-request-id`. With the `context-scope` feature
//!   enabled, it also installs the context as the ambient
//!   [`RequestContext::current`](RequestContext) for the handler future.
//! * [`Context`] — a [`FromRequest`] extractor that hands the stored
//!   [`RequestContext`] to handlers (`async fn handler(ctx: Context)`),
//!   falling back to a fresh default if, for some reason, none is present.
//!
//! ```no_run
//! use actix_web::{web, App, HttpResponse};
//! use klauthed_web::context::{Context, RequestContextMiddleware};
//!
//! async fn handler(ctx: Context) -> HttpResponse {
//!     HttpResponse::Ok().body(ctx.request_id().to_string())
//! }
//!
//! let app = App::new()
//!     .wrap(RequestContextMiddleware::new())
//!     .route("/", web::get().to(handler));
//! ```

use std::future::{Ready, ready};
use std::ops::Deref;
use std::rc::Rc;
use std::task::{Context as TaskContext, Poll};

use actix_web::dev::{Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::http::header::{HeaderName, HeaderValue};
use actix_web::{Error, FromRequest, HttpMessage, HttpRequest};
use futures_util::future::LocalBoxFuture;
use klauthed_core::context::{RequestContext, RequestId};

/// Header carrying the request id (generated when absent).
pub const REQUEST_ID_HEADER: &str = "x-request-id";
/// Header carrying an inbound correlation / trace id.
pub const CORRELATION_ID_HEADER: &str = "x-correlation-id";
/// Header carrying the tenant identifier.
pub const TENANT_ID_HEADER: &str = "x-tenant-id";
/// Standard header used to derive the request locale.
pub const ACCEPT_LANGUAGE_HEADER: &str = "accept-language";

/// Build a [`RequestContext`] from a request's headers.
///
/// `x-request-id` is parsed into a [`RequestId`] when it is a valid UUID/ULID,
/// otherwise (and when absent) a fresh id is minted. `x-correlation-id`,
/// `x-tenant-id`, and the first language of `Accept-Language` populate the
/// matching fields when present and non-empty.
fn context_from_request(req: &ServiceRequest) -> RequestContext {
    let headers = req.headers();

    let mut ctx = RequestContext::new();

    if let Some(raw) = header_str(headers.get(REQUEST_ID_HEADER))
        && let Ok(id) = raw.parse::<RequestId>()
    {
        ctx = ctx.with_request_id(id);
    }

    if let Some(correlation) = header_str(headers.get(CORRELATION_ID_HEADER)) {
        ctx = ctx.with_correlation_id(correlation);
    }

    if let Some(tenant) = header_str(headers.get(TENANT_ID_HEADER)) {
        ctx = ctx.with_tenant(tenant);
    }

    if let Some(locale) = header_str(headers.get(ACCEPT_LANGUAGE_HEADER)).and_then(first_language) {
        ctx = ctx.with_locale(locale);
    }

    ctx
}

/// Borrow a header value as a trimmed, non-empty UTF-8 string.
fn header_str(value: Option<&HeaderValue>) -> Option<&str> {
    let raw = value?.to_str().ok()?.trim();
    (!raw.is_empty()).then_some(raw)
}

/// The first language tag from an `Accept-Language` value (ignoring q-weights).
///
/// E.g. `fr-CH, fr;q=0.9, en;q=0.8` → `fr-CH`. The wildcard `*` is ignored.
fn first_language(accept_language: &str) -> Option<&str> {
    accept_language
        .split(',')
        .map(|part| part.split(';').next().unwrap_or(part).trim())
        .find(|tag| !tag.is_empty() && *tag != "*")
}

// ── Middleware ────────────────────────────────────────────────────────────────

/// actix [`Transform`] that establishes a [`RequestContext`] per request.
///
/// Wrap it on an `App`/scope with `.wrap(RequestContextMiddleware::new())`. The
/// context is placed in the request extensions (readable via the [`Context`]
/// extractor) and the resolved request id is set on the response as
/// `x-request-id`.
#[derive(Debug, Clone, Default)]
pub struct RequestContextMiddleware {
    _priv: (),
}

impl RequestContextMiddleware {
    /// Construct the middleware.
    pub fn new() -> Self {
        Self::default()
    }
}

impl<S, B> Transform<S, ServiceRequest> for RequestContextMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Transform = RequestContextService<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(RequestContextService { service: Rc::new(service) }))
    }
}

/// The [`Service`] produced by [`RequestContextMiddleware`].
pub struct RequestContextService<S> {
    service: Rc<S>,
}

impl<S, B> Service<ServiceRequest> for RequestContextService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&self, cx: &mut TaskContext<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let ctx = context_from_request(&req);
        let request_id = ctx.request_id();

        // Make the context available to the FromRequest extractor.
        req.extensions_mut().insert(ctx.clone());

        let service = Rc::clone(&self.service);

        Box::pin(async move {
            let mut res = call_in_context(service, req, ctx).await?;

            if let Ok(value) = HeaderValue::from_str(&request_id.to_string()) {
                res.headers_mut().insert(HeaderName::from_static(REQUEST_ID_HEADER), value);
            }

            Ok(res)
        })
    }
}

/// Drive the inner service, installing `ctx` as the ambient context when the
/// `context-scope` feature is enabled.
#[cfg(feature = "context-scope")]
async fn call_in_context<S, B>(
    service: Rc<S>,
    req: ServiceRequest,
    ctx: RequestContext,
) -> Result<ServiceResponse<B>, Error>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
{
    ctx.scope(service.call(req)).await
}

#[cfg(not(feature = "context-scope"))]
async fn call_in_context<S, B>(
    service: Rc<S>,
    req: ServiceRequest,
    _ctx: RequestContext,
) -> Result<ServiceResponse<B>, Error>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
{
    service.call(req).await
}

// ── Extractor ─────────────────────────────────────────────────────────────────

/// Extractor handing the per-request [`RequestContext`] to handlers.
///
/// It reads the context the [`RequestContextMiddleware`] stored in the request
/// extensions. If none is present (e.g. the middleware was not mounted), it
/// yields a fresh default context rather than failing the request.
///
/// Deref to [`RequestContext`], so all of its accessors are available directly,
/// and [`Context::into_inner`] takes ownership of the underlying context.
#[derive(Debug, Clone)]
pub struct Context(RequestContext);

impl Context {
    /// Consume the extractor and return the owned [`RequestContext`].
    pub fn into_inner(self) -> RequestContext {
        self.0
    }
}

impl Deref for Context {
    type Target = RequestContext;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<Context> for RequestContext {
    fn from(ctx: Context) -> Self {
        ctx.0
    }
}

impl FromRequest for Context {
    type Error = Error;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _payload: &mut actix_web::dev::Payload) -> Self::Future {
        let ctx = req.extensions().get::<RequestContext>().cloned().unwrap_or_default();
        ready(Ok(Context(ctx)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{App, HttpResponse, test, web};

    #[std::prelude::v1::test]
    fn first_language_picks_first_tag() {
        assert_eq!(first_language("fr-CH, fr;q=0.9, en;q=0.8"), Some("fr-CH"));
        assert_eq!(first_language("en-US"), Some("en-US"));
        assert_eq!(first_language("*"), None);
        assert_eq!(first_language(""), None);
        assert_eq!(first_language(" , de"), Some("de"));
    }

    #[std::prelude::v1::test]
    fn header_str_trims_and_rejects_empty() {
        assert_eq!(header_str(Some(&HeaderValue::from_static("  acme "))), Some("acme"));
        assert_eq!(header_str(Some(&HeaderValue::from_static("   "))), None);
        assert_eq!(header_str(None), None);
    }

    async fn echo(ctx: Context) -> HttpResponse {
        let body = serde_json::json!({
            "request_id": ctx.request_id().to_string(),
            "correlation_id": ctx.correlation_id(),
            "tenant": ctx.tenant(),
            "locale": ctx.locale(),
        });
        HttpResponse::Ok().json(body)
    }

    #[actix_web::test]
    async fn middleware_propagates_headers_into_context() {
        let app = test::init_service(
            App::new().wrap(RequestContextMiddleware::new()).route("/", web::get().to(echo)),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/")
            .insert_header((CORRELATION_ID_HEADER, "corr-123"))
            .insert_header((TENANT_ID_HEADER, "acme"))
            .insert_header((ACCEPT_LANGUAGE_HEADER, "tr-TR, en;q=0.8"))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        // Response echoes a request id.
        let echoed = resp
            .headers()
            .get(REQUEST_ID_HEADER)
            .expect("x-request-id on response")
            .to_str()
            .unwrap()
            .to_owned();
        assert!(!echoed.is_empty());

        let body = test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["correlation_id"], "corr-123");
        assert_eq!(json["tenant"], "acme");
        assert_eq!(json["locale"], "tr-TR");
        assert_eq!(json["request_id"], echoed);
    }

    #[actix_web::test]
    async fn middleware_honors_inbound_request_id() {
        let incoming = RequestId::new().to_string();

        let app = test::init_service(
            App::new().wrap(RequestContextMiddleware::new()).route("/", web::get().to(echo)),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/")
            .insert_header((REQUEST_ID_HEADER, incoming.clone()))
            .to_request();

        let resp = test::call_service(&app, req).await;
        let echoed = resp.headers().get(REQUEST_ID_HEADER).unwrap().to_str().unwrap();
        assert_eq!(echoed, incoming);
    }

    #[actix_web::test]
    async fn extractor_falls_back_to_default_without_middleware() {
        let app = test::init_service(App::new().route("/", web::get().to(echo))).await;

        let req = test::TestRequest::get().uri("/").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let body = test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        // A fresh default context still has a (non-empty) request id.
        assert!(!json["request_id"].as_str().unwrap().is_empty());
        assert!(json["tenant"].is_null());
    }

    #[cfg(feature = "context-scope")]
    #[actix_web::test]
    async fn middleware_installs_ambient_context() {
        async fn ambient() -> HttpResponse {
            match RequestContext::try_current() {
                Some(ctx) => HttpResponse::Ok().body(ctx.request_id().to_string()),
                None => HttpResponse::InternalServerError().body("no ambient context"),
            }
        }

        let app = test::init_service(
            App::new().wrap(RequestContextMiddleware::new()).route("/", web::get().to(ambient)),
        )
        .await;

        let req = test::TestRequest::get().uri("/").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let echoed = resp.headers().get(REQUEST_ID_HEADER).unwrap().to_str().unwrap().to_owned();
        let body = test::read_body(resp).await;
        assert_eq!(String::from_utf8(body.to_vec()).unwrap(), echoed);
    }
}
