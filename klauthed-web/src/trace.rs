//! OpenTelemetry request-tracing middleware (`otel` feature).
//!
//! [`RequestTracing`] opens a span per HTTP request (method, path, response
//! status), links it to the caller's trace by extracting the inbound W3C
//! `traceparent`, and records the status when the response is ready. With the
//! OTLP pipeline installed by `klauthed_observability::init`, these spans export
//! as distributed traces. Propagate the context into outbound calls with
//! [`klauthed_observability::propagation::inject_current`].
//!
//! ```no_run
//! use actix_web::{App, web, HttpResponse};
//! use klauthed_web::RequestTracing;
//!
//! let app = App::new()
//!     .wrap(RequestTracing::new())
//!     .route("/", web::get().to(|| async { HttpResponse::Ok().finish() }));
//! ```

use std::future::{Ready, ready};
use std::rc::Rc;
use std::task::{Context as TaskContext, Poll};

use actix_web::Error;
use actix_web::dev::{Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::http::header::HeaderName;
use futures_util::future::LocalBoxFuture;
use opentelemetry::propagation::Extractor;
use tracing::Instrument;
use tracing_opentelemetry::OpenTelemetrySpanExt;

/// actix [`Transform`] that traces each request as an OpenTelemetry span.
///
/// Wrap it on an `App`/scope with `.wrap(RequestTracing::new())`.
#[derive(Debug, Clone, Default)]
pub struct RequestTracing {
    _priv: (),
}

impl RequestTracing {
    /// Construct the middleware.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl<S, B> Transform<S, ServiceRequest> for RequestTracing
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Transform = RequestTracingService<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(RequestTracingService { service: Rc::new(service) }))
    }
}

/// The [`Service`] produced by [`RequestTracing`].
pub struct RequestTracingService<S> {
    service: Rc<S>,
}

impl<S, B> Service<ServiceRequest> for RequestTracingService<S>
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
        let span = tracing::info_span!(
            "http.request",
            "otel.name" = %format!("{} {}", req.method(), req.path()),
            "otel.kind" = "server",
            "http.request.method" = %req.method(),
            "url.path" = %req.path(),
            "http.response.status_code" = tracing::field::Empty,
        );
        // Link this server span to the caller's trace via the inbound W3C context.
        // Best-effort: `set_parent` errors only when no OTel layer is installed.
        let parent = klauthed_observability::propagation::extract(&HeaderExtractor(req.headers()));
        let _ = span.set_parent(parent);

        let service = Rc::clone(&self.service);
        Box::pin(
            async move {
                let res = service.call(req).await?;
                tracing::Span::current().record("http.response.status_code", res.status().as_u16());
                Ok(res)
            }
            .instrument(span),
        )
    }
}

/// [`Extractor`] over actix's request header map (for W3C context extraction).
struct HeaderExtractor<'a>(&'a actix_web::http::header::HeaderMap);

impl Extractor for HeaderExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|value| value.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(HeaderName::as_str).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{App, HttpResponse, test, web};

    async fn ok() -> HttpResponse {
        HttpResponse::Ok().finish()
    }

    #[actix_web::test]
    async fn passes_requests_through_with_and_without_traceparent() {
        let app = test::init_service(
            App::new().wrap(RequestTracing::new()).route("/", web::get().to(ok)),
        )
        .await;

        // Plain request.
        let resp = test::call_service(&app, test::TestRequest::get().uri("/").to_request()).await;
        assert!(resp.status().is_success());

        // Request carrying an inbound W3C traceparent (parent context extracted).
        let req = test::TestRequest::get()
            .uri("/")
            .insert_header((
                "traceparent",
                "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01",
            ))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    }
}
