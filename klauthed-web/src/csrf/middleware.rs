//! The [`Csrf`] actix [`Transform`] and its [`CsrfService`].

use std::future::{Ready, ready};
use std::rc::Rc;
use std::task::{Context as TaskContext, Poll};

use actix_web::body::EitherBody;
use actix_web::cookie::Cookie;
use actix_web::dev::{Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::http::Method;
use actix_web::http::header::{AUTHORIZATION, HeaderMap, HeaderValue, SET_COOKIE};
use actix_web::{Error, ResponseError};
use futures_util::future::LocalBoxFuture;
use klauthed_security::{SecurityError, constant_time_eq, random_token};

use super::CsrfConfig;
use crate::error::AppError;

/// Cross-Site Request Forgery protection via the double-submit-cookie pattern
/// (an actix [`Transform`]).
///
/// On every **unsafe** request (`POST`, `PUT`, `PATCH`, `DELETE`, …) the value
/// of the CSRF cookie must equal the value echoed in the configured request
/// header, compared in constant time. Safe requests (`GET`, `HEAD`, `OPTIONS`,
/// `TRACE`) always pass and — with [`auto_issue`](CsrfConfig::auto_issue) — seed
/// a fresh cookie when none is present. Requests carrying an
/// `Authorization: Bearer` token are skipped by default (they aren't exposed to
/// CSRF). A failed check returns `403 Forbidden`.
///
/// ```no_run
/// use actix_web::App;
/// use klauthed_web::csrf::Csrf;
///
/// let _app = App::new().wrap(Csrf::new());
/// ```
///
/// The cookie is **not** `HttpOnly` — the double-submit pattern requires client
/// JavaScript to read it and copy it into the request header. Pair it with a
/// `SameSite=Lax`/`Strict` cookie (the default) for defense in depth.
#[derive(Clone)]
pub struct Csrf {
    config: Rc<CsrfConfig>,
}

impl Csrf {
    /// Build with the [`CsrfConfig::default`] double-submit policy.
    #[must_use]
    pub fn new() -> Self {
        Self::from_config(CsrfConfig::default())
    }

    /// Build from an explicit [`CsrfConfig`].
    #[must_use]
    pub fn from_config(config: CsrfConfig) -> Self {
        Self { config: Rc::new(config) }
    }

    /// Mint a fresh token and build the cookie that carries it.
    ///
    /// Use this from a handler to rotate the token (e.g. right after a successful
    /// login): add the returned cookie to the response, and the client's next
    /// unsafe request must echo its value.
    ///
    /// # Errors
    ///
    /// Returns [`SecurityError`] if the system RNG fails.
    pub fn issue_cookie(&self) -> Result<Cookie<'static>, SecurityError> {
        build_issued_cookie(&self.config)
    }
}

impl Default for Csrf {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether a method is "safe" (read-only) and therefore exempt from the check.
fn is_safe_method(method: &Method) -> bool {
    matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS | Method::TRACE)
}

/// Whether the request authenticates with an `Authorization: Bearer` token.
fn has_bearer(headers: &HeaderMap) -> bool {
    headers.get(AUTHORIZATION).and_then(|value| value.to_str().ok()).is_some_and(|value| {
        value.trim_start().get(..7).is_some_and(|p| p.eq_ignore_ascii_case("bearer "))
    })
}

/// Build the cookie for a freshly minted token from `config`.
fn build_issued_cookie(config: &CsrfConfig) -> Result<Cookie<'static>, SecurityError> {
    let token = random_token(config.token_bytes)?;
    Ok(Cookie::build(config.cookie_name.clone(), token)
        .path(config.cookie_path.clone())
        .secure(config.secure)
        // Must be readable by JS so the client can echo it in the header.
        .http_only(false)
        .same_site(config.same_site.into())
        .finish())
}

/// Append a `Set-Cookie` header for `cookie` to `headers`.
fn append_set_cookie(headers: &mut HeaderMap, cookie: &Cookie<'_>) {
    if let Ok(value) = HeaderValue::from_str(&cookie.to_string()) {
        headers.append(SET_COOKIE, value);
    }
}

impl<S, B> Transform<S, ServiceRequest> for Csrf
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Transform = CsrfService<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(CsrfService { service: Rc::new(service), config: Rc::clone(&self.config) }))
    }
}

/// The [`Service`] produced by [`Csrf`].
pub struct CsrfService<S> {
    service: Rc<S>,
    config: Rc<CsrfConfig>,
}

impl<S, B> Service<ServiceRequest> for CsrfService<S>
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
        let config = Rc::clone(&self.config);
        let service = Rc::clone(&self.service);

        // Read what we need before the request is consumed by the inner service.
        let safe = is_safe_method(req.method());
        let cookie_token = req.cookie(&config.cookie_name).map(|c| c.value().to_owned());
        let header_token = req
            .headers()
            .get(config.header_name.as_str())
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let bearer_exempt = config.skip_bearer && has_bearer(req.headers());

        if safe {
            let need_issue = config.auto_issue && cookie_token.is_none();
            return Box::pin(async move {
                let mut res = service.call(req).await?.map_into_left_body();
                if need_issue {
                    match build_issued_cookie(&config) {
                        Ok(cookie) => append_set_cookie(res.headers_mut(), &cookie),
                        Err(error) => {
                            tracing::warn!(%error, "failed to mint CSRF token; cookie not issued");
                        }
                    }
                }
                Ok(res)
            });
        }

        if bearer_exempt {
            return Box::pin(async move {
                service.call(req).await.map(ServiceResponse::map_into_left_body)
            });
        }

        let valid = match (&cookie_token, &header_token) {
            (Some(cookie), Some(header)) if !cookie.is_empty() => {
                constant_time_eq(cookie.as_bytes(), header.as_bytes())
            }
            _ => false,
        };

        if valid {
            Box::pin(
                async move { service.call(req).await.map(ServiceResponse::map_into_left_body) },
            )
        } else {
            Box::pin(async move {
                let err = AppError::forbidden("missing or invalid CSRF token");
                Ok(req.into_response(err.error_response()).map_into_right_body())
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::cookie::Cookie as TestCookie;
    use actix_web::{App, HttpResponse, test, web};

    async fn ok() -> HttpResponse {
        HttpResponse::Ok().finish()
    }

    fn app_uri() -> actix_web::test::TestRequest {
        test::TestRequest::default()
    }

    #[std::prelude::v1::test]
    fn safe_methods_are_exempt() {
        assert!(is_safe_method(&Method::GET));
        assert!(is_safe_method(&Method::HEAD));
        assert!(is_safe_method(&Method::OPTIONS));
        assert!(!is_safe_method(&Method::POST));
        assert!(!is_safe_method(&Method::DELETE));
    }

    #[actix_web::test]
    async fn get_auto_issues_cookie() {
        let app =
            test::init_service(App::new().wrap(Csrf::new()).route("/", web::get().to(ok))).await;
        let resp = test::call_service(&app, app_uri().uri("/").to_request()).await;
        assert!(resp.status().is_success());
        let set_cookie = resp.headers().get(SET_COOKIE).expect("Set-Cookie present");
        assert!(set_cookie.to_str().unwrap().starts_with("csrf_token="));
    }

    #[actix_web::test]
    async fn post_without_token_is_forbidden() {
        let app =
            test::init_service(App::new().wrap(Csrf::new()).route("/", web::post().to(ok))).await;
        let resp =
            test::call_service(&app, app_uri().method(Method::POST).uri("/").to_request()).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);
    }

    #[actix_web::test]
    async fn post_with_matching_token_passes() {
        let app =
            test::init_service(App::new().wrap(Csrf::new()).route("/", web::post().to(ok))).await;
        let req = app_uri()
            .method(Method::POST)
            .uri("/")
            .cookie(TestCookie::new("csrf_token", "tok-123"))
            .insert_header(("x-csrf-token", "tok-123"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    }

    #[actix_web::test]
    async fn post_with_mismatched_token_is_forbidden() {
        let app =
            test::init_service(App::new().wrap(Csrf::new()).route("/", web::post().to(ok))).await;
        let req = app_uri()
            .method(Method::POST)
            .uri("/")
            .cookie(TestCookie::new("csrf_token", "tok-123"))
            .insert_header(("x-csrf-token", "different"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);
    }

    #[actix_web::test]
    async fn bearer_requests_are_skipped() {
        let app =
            test::init_service(App::new().wrap(Csrf::new()).route("/", web::post().to(ok))).await;
        let req = app_uri()
            .method(Method::POST)
            .uri("/")
            .insert_header(("authorization", "Bearer abc.def.ghi"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    }

    #[actix_web::test]
    async fn bearer_skip_can_be_disabled() {
        let csrf = Csrf::from_config(CsrfConfig::default().skip_bearer(false));
        let app = test::init_service(App::new().wrap(csrf).route("/", web::post().to(ok))).await;
        let req = app_uri()
            .method(Method::POST)
            .uri("/")
            .insert_header(("authorization", "Bearer abc.def.ghi"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);
    }

    #[std::prelude::v1::test]
    fn issue_cookie_has_expected_attributes() {
        let cookie = Csrf::new().issue_cookie().unwrap();
        assert_eq!(cookie.name(), "csrf_token");
        assert!(!cookie.value().is_empty());
        assert_eq!(cookie.http_only(), Some(false));
        assert_eq!(cookie.secure(), Some(true));
    }

    #[std::prelude::v1::test]
    fn detects_bearer_case_insensitively() {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_static("bearer xyz"));
        assert!(has_bearer(&headers));
        headers.insert(AUTHORIZATION, HeaderValue::from_static("Basic xyz"));
        assert!(!has_bearer(&headers));
    }
}
