//! The [`SecurityHeaders`] actix [`Transform`] and its [`SecurityHeadersService`].

use std::future::{Ready, ready};
use std::rc::Rc;
use std::task::{Context as TaskContext, Poll};

use actix_web::Error;
use actix_web::dev::{Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::http::header::{HeaderName, HeaderValue};
use futures_util::future::LocalBoxFuture;

use super::SecurityHeadersConfig;

/// Middleware that adds security-related response headers (an actix [`Transform`]).
///
/// The configured headers are rendered once when the middleware is built, then
/// applied to every response. A header is only set when the handler has not
/// already produced one, so a route can override (e.g. a relaxed CSP on an HTML
/// page) without fighting the middleware.
///
/// ```no_run
/// use actix_web::App;
/// use klauthed_web::headers::SecurityHeaders;
///
/// // Strict defaults (deny framing, nosniff, HSTS, locked-down CSP, …).
/// let _app = App::new().wrap(SecurityHeaders::new());
/// ```
///
/// Mount it as one of the outermost layers so the headers cover error responses
/// produced by inner middleware too.
#[derive(Clone)]
pub struct SecurityHeaders {
    headers: Rc<Vec<(HeaderName, HeaderValue)>>,
}

impl SecurityHeaders {
    /// Build with the strict [`SecurityHeadersConfig::default`] policy.
    #[must_use]
    pub fn new() -> Self {
        Self::from_config(&SecurityHeadersConfig::default())
    }

    /// Build from an explicit [`SecurityHeadersConfig`].
    #[must_use]
    pub fn from_config(config: &SecurityHeadersConfig) -> Self {
        Self { headers: Rc::new(config.header_pairs()) }
    }
}

impl Default for SecurityHeaders {
    fn default() -> Self {
        Self::new()
    }
}

impl<S, B> Transform<S, ServiceRequest> for SecurityHeaders
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Transform = SecurityHeadersService<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(SecurityHeadersService {
            service: Rc::new(service),
            headers: Rc::clone(&self.headers),
        }))
    }
}

/// The [`Service`] produced by [`SecurityHeaders`].
pub struct SecurityHeadersService<S> {
    service: Rc<S>,
    headers: Rc<Vec<(HeaderName, HeaderValue)>>,
}

impl<S, B> Service<ServiceRequest> for SecurityHeadersService<S>
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
        let headers = Rc::clone(&self.headers);
        let fut = self.service.call(req);

        Box::pin(async move {
            let mut res = fut.await?;
            let map = res.headers_mut();
            for (name, value) in headers.iter() {
                // Don't clobber a header a handler set deliberately.
                if !map.contains_key(name) {
                    map.insert(name.clone(), value.clone());
                }
            }
            Ok(res)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::headers::{FrameOptions, SecurityHeadersConfig};
    use actix_web::{App, HttpResponse, test, web};

    async fn ok() -> HttpResponse {
        HttpResponse::Ok().finish()
    }

    #[actix_web::test]
    async fn applies_default_headers() {
        let app = test::init_service(
            App::new().wrap(SecurityHeaders::new()).route("/", web::get().to(ok)),
        )
        .await;

        let resp = test::call_service(&app, test::TestRequest::get().uri("/").to_request()).await;
        let h = resp.headers();
        assert_eq!(h.get("x-content-type-options").unwrap(), "nosniff");
        assert_eq!(h.get("x-frame-options").unwrap(), "DENY");
        assert!(h.get("strict-transport-security").unwrap().to_str().unwrap().contains("max-age="));
        assert!(h.contains_key("content-security-policy"));
        assert_eq!(h.get("cross-origin-opener-policy").unwrap(), "same-origin");
    }

    #[actix_web::test]
    async fn does_not_override_a_handler_set_header() {
        async fn custom_csp() -> HttpResponse {
            HttpResponse::Ok()
                .insert_header(("content-security-policy", "default-src 'self'"))
                .finish()
        }

        let app = test::init_service(
            App::new().wrap(SecurityHeaders::new()).route("/", web::get().to(custom_csp)),
        )
        .await;

        let resp = test::call_service(&app, test::TestRequest::get().uri("/").to_request()).await;
        assert_eq!(resp.headers().get("content-security-policy").unwrap(), "default-src 'self'");
    }

    #[actix_web::test]
    async fn honors_config_without_hsts() {
        let cfg = SecurityHeadersConfig::default()
            .without_hsts()
            .with_frame_options(FrameOptions::SameOrigin);
        let app = test::init_service(
            App::new().wrap(SecurityHeaders::from_config(&cfg)).route("/", web::get().to(ok)),
        )
        .await;

        let resp = test::call_service(&app, test::TestRequest::get().uri("/").to_request()).await;
        assert!(!resp.headers().contains_key("strict-transport-security"));
        assert_eq!(resp.headers().get("x-frame-options").unwrap(), "SAMEORIGIN");
    }
}
