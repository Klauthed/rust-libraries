//! The per-request [`DynamicCors`] middleware that checks each `Origin` against
//! static platform origins and a [`CorsOriginRegistry`].

use std::collections::HashSet;
use std::future::{ready, Ready};
use std::rc::Rc;
use std::sync::Arc;
use std::task::{Context as TaskContext, Poll};

use actix_web::body::BoxBody;
use actix_web::dev::{Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::http::header::{
    HeaderMap, HeaderValue, ACCESS_CONTROL_ALLOW_CREDENTIALS, ACCESS_CONTROL_ALLOW_HEADERS,
    ACCESS_CONTROL_ALLOW_METHODS, ACCESS_CONTROL_ALLOW_ORIGIN, ACCESS_CONTROL_EXPOSE_HEADERS,
    ACCESS_CONTROL_MAX_AGE, ORIGIN, VARY,
};
use actix_web::http::Method;
use actix_web::{Error, HttpResponse};
use futures_util::future::LocalBoxFuture;

use super::config::CorsConfig;
use super::registry::{CorsOriginRegistry, InMemoryOriginRegistry};

/// Per-request dynamic CORS middleware.
///
/// On each request it inspects the `Origin` header and checks it against:
/// 1. `config.allowed_origins` — your platform's own frontends (O(1), no I/O)
/// 2. A [`CorsOriginRegistry`] — tenant-registered domains (async lookup)
///
/// If the origin is allowed:
/// * `OPTIONS` preflight → short-circuits with `204 No Content` + preflight
///   headers (inner service is never called).
/// * Other methods → calls the inner service, then adds CORS headers to the
///   response.
///
/// If the origin is absent or not allowed the request passes through
/// untouched. `Vary: Origin` is always appended on responses for requests
/// that carry an `Origin` header so HTTP caches handle the variance correctly.
///
/// See the [module-level docs](super) for wiring instructions and security
/// notes.
#[derive(Clone)]
pub struct DynamicCors {
    /// Origins checked without any I/O (e.g. your own auth frontend).
    static_origins: Arc<HashSet<String>>,
    /// Dynamic registry (tenant domains, backed by DB + cache).
    registry: Arc<dyn CorsOriginRegistry>,
    /// CORS policy shared across all allowed responses.
    config: Arc<CorsConfig>,
}

impl DynamicCors {
    /// Build with `config` (policy + static origins) and a `registry` for
    /// tenant domains.
    ///
    /// `config.allowed_origins` becomes the set of always-allowed platform
    /// origins (checked before the registry). The rest of `config` drives the
    /// CORS headers that are sent.
    pub fn new(config: CorsConfig, registry: Arc<dyn CorsOriginRegistry>) -> Self {
        let static_set: HashSet<String> = config.allowed_origins.iter().cloned().collect();
        Self {
            static_origins: Arc::new(static_set),
            registry,
            config: Arc::new(config),
        }
    }

    /// Convenience: use an [`InMemoryOriginRegistry`] (tenant origins added
    /// at runtime via `registry.insert()`).
    pub fn with_memory_registry(
        config: CorsConfig,
    ) -> (Self, Arc<InMemoryOriginRegistry>) {
        let reg = Arc::new(InMemoryOriginRegistry::new());
        let cors = Self::new(config, Arc::clone(&reg) as Arc<dyn CorsOriginRegistry>);
        (cors, reg)
    }
}

impl<S, B> Transform<S, ServiceRequest> for DynamicCors
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: actix_web::body::MessageBody + 'static,
{
    type Response = ServiceResponse<BoxBody>;
    type Error = Error;
    type Transform = DynamicCorsService<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(DynamicCorsService {
            service: Rc::new(service),
            static_origins: Arc::clone(&self.static_origins),
            registry: Arc::clone(&self.registry),
            config: Arc::clone(&self.config),
        }))
    }
}

/// The [`Service`] produced by [`DynamicCors`].
pub struct DynamicCorsService<S> {
    service: Rc<S>,
    static_origins: Arc<HashSet<String>>,
    registry: Arc<dyn CorsOriginRegistry>,
    config: Arc<CorsConfig>,
}

impl<S, B> Service<ServiceRequest> for DynamicCorsService<S>
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
        let static_origins = Arc::clone(&self.static_origins);
        let registry = Arc::clone(&self.registry);
        let config = Arc::clone(&self.config);
        let service = Rc::clone(&self.service);

        // Read the Origin header before moving `req`.
        let origin: Option<String> = req
            .headers()
            .get(ORIGIN)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);

        Box::pin(async move {
            // Determine whether this Origin is allowed (O(1) static check
            // first, async registry only on a miss).
            let allowed_origin: Option<String> = match origin {
                None => None, // No Origin header → same-origin, no CORS needed.
                Some(ref o) => {
                    let allowed = static_origins.contains(o.as_str())
                        || registry.is_allowed(o).await;
                    if allowed { Some(o.clone()) } else { None }
                }
            };

            // CORS preflight: answer OPTIONS immediately, never call inner service.
            if *req.method() == Method::OPTIONS {
                let mut resp = HttpResponse::NoContent()
                    .finish()
                    .map_into_boxed_body();

                // Even for rejected origins we add Vary so caches don't serve
                // a preflight response to a different origin.
                resp.headers_mut()
                    .append(VARY, HeaderValue::from_static("Origin"));

                if let Some(ref o) = allowed_origin {
                    set_cors_headers(resp.headers_mut(), o, &config, true);
                }
                return Ok(req.into_response(resp));
            }

            // Non-preflight: call the inner service.
            let mut res = service
                .call(req)
                .await
                .map(ServiceResponse::map_into_boxed_body)?;

            // Vary: Origin on every response that had an Origin header so
            // caches (CDN, browser) handle origin variance correctly.
            if origin.is_some() {
                res.headers_mut()
                    .append(VARY, HeaderValue::from_static("Origin"));
            }

            if let Some(ref o) = allowed_origin {
                set_cors_headers(res.headers_mut(), o, &config, false);
            }

            Ok(res)
        })
    }
}

/// Write CORS headers onto `headers`. `is_preflight` enables the extra
/// method/header negotiation headers that only belong on `OPTIONS` responses.
fn set_cors_headers(headers: &mut HeaderMap, origin: &str, config: &CorsConfig, is_preflight: bool) {
    // Echo the exact origin — never use `*` with credentials.
    if let Ok(v) = HeaderValue::from_str(origin) {
        headers.insert(ACCESS_CONTROL_ALLOW_ORIGIN, v);
    }

    if config.allow_credentials {
        headers.insert(
            ACCESS_CONTROL_ALLOW_CREDENTIALS,
            HeaderValue::from_static("true"),
        );
    }

    if !config.expose_headers.is_empty()
        && let Ok(v) = HeaderValue::from_str(&config.expose_headers.join(", "))
    {
        headers.insert(ACCESS_CONTROL_EXPOSE_HEADERS, v);
    }

    // Preflight-only: method + header negotiation and cache lifetime.
    if is_preflight {
        if let Ok(v) = HeaderValue::from_str(&config.allowed_methods.join(", ")) {
            headers.insert(ACCESS_CONTROL_ALLOW_METHODS, v);
        }
        if let Ok(v) = HeaderValue::from_str(&config.allowed_headers.join(", ")) {
            headers.insert(ACCESS_CONTROL_ALLOW_HEADERS, v);
        }
        if let Some(secs) = config.max_age_secs
            && let Ok(v) = HeaderValue::from_str(&secs.to_string())
        {
            headers.insert(ACCESS_CONTROL_MAX_AGE, v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::http::StatusCode;
    use actix_web::test as http_test;
    use actix_web::{web, App, HttpResponse};

    fn platform_config() -> CorsConfig {
        CorsConfig {
            allowed_origins: vec!["https://auth.platform.com".into()],
            allow_credentials: true,
            ..CorsConfig::default()
        }
    }

    async fn ok_handler() -> HttpResponse {
        HttpResponse::Ok().body("hello")
    }

    #[actix_web::test]
    async fn static_platform_origin_is_allowed_without_registry() {
        let reg = Arc::new(InMemoryOriginRegistry::new()); // empty registry
        let cors = DynamicCors::new(platform_config(), reg);
        let app = http_test::init_service(
            App::new().wrap(cors).route("/", web::get().to(ok_handler)),
        )
        .await;

        let req = http_test::TestRequest::get()
            .uri("/")
            .insert_header(("Origin", "https://auth.platform.com"))
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let acao = resp
            .headers()
            .get("access-control-allow-origin")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert_eq!(acao, "https://auth.platform.com");

        let vary = resp
            .headers()
            .get("vary")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(vary.contains("Origin"));
    }

    #[actix_web::test]
    async fn tenant_origin_from_registry_is_allowed() {
        let reg = Arc::new(InMemoryOriginRegistry::new());
        reg.insert("https://login.acme.com");
        let cors = DynamicCors::new(platform_config(), Arc::clone(&reg) as Arc<dyn CorsOriginRegistry>);
        let app = http_test::init_service(
            App::new().wrap(cors).route("/", web::get().to(ok_handler)),
        )
        .await;

        let req = http_test::TestRequest::get()
            .uri("/")
            .insert_header(("Origin", "https://login.acme.com"))
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers()
                .get("access-control-allow-origin")
                .and_then(|v| v.to_str().ok()),
            Some("https://login.acme.com")
        );
    }

    #[actix_web::test]
    async fn unknown_origin_gets_no_cors_headers() {
        let reg = Arc::new(InMemoryOriginRegistry::new());
        let cors = DynamicCors::new(platform_config(), reg);
        let app = http_test::init_service(
            App::new().wrap(cors).route("/", web::get().to(ok_handler)),
        )
        .await;

        let req = http_test::TestRequest::get()
            .uri("/")
            .insert_header(("Origin", "https://attacker.example.com"))
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK); // request succeeds (CORS is client-enforced)
        assert!(resp.headers().get("access-control-allow-origin").is_none());
        // Vary: Origin still set so caches handle it correctly.
        let vary = resp.headers().get("vary").and_then(|v| v.to_str().ok()).unwrap_or("");
        assert!(vary.contains("Origin"));
    }

    #[actix_web::test]
    async fn options_preflight_for_allowed_origin_returns_204() {
        let reg = Arc::new(InMemoryOriginRegistry::with_origins(["https://login.acme.com"]));
        let cors = DynamicCors::new(platform_config(), reg);
        let app = http_test::init_service(
            App::new().wrap(cors).route("/api", web::get().to(ok_handler)),
        )
        .await;

        let req = http_test::TestRequest::default()
            .method(Method::OPTIONS)
            .uri("/api")
            .insert_header(("Origin", "https://login.acme.com"))
            .insert_header(("Access-Control-Request-Method", "POST"))
            .insert_header(("Access-Control-Request-Headers", "content-type,authorization"))
            .to_request();
        let resp = http_test::call_service(&app, req).await;

        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            resp.headers()
                .get("access-control-allow-origin")
                .and_then(|v| v.to_str().ok()),
            Some("https://login.acme.com")
        );
        // Preflight must include methods and headers.
        assert!(resp.headers().contains_key("access-control-allow-methods"));
        assert!(resp.headers().contains_key("access-control-allow-headers"));
    }

    #[actix_web::test]
    async fn options_preflight_for_unknown_origin_passes_through_with_vary() {
        let reg = Arc::new(InMemoryOriginRegistry::new());
        let cors = DynamicCors::new(platform_config(), reg);
        let app = http_test::init_service(
            App::new().wrap(cors).route("/", web::get().to(ok_handler)),
        )
        .await;

        let req = http_test::TestRequest::default()
            .method(Method::OPTIONS)
            .uri("/")
            .insert_header(("Origin", "https://unknown.example.com"))
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        // Rejected origin → 204 with Vary but no ACAO header.
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        assert!(resp.headers().get("access-control-allow-origin").is_none());
        let vary = resp.headers().get("vary").and_then(|v| v.to_str().ok()).unwrap_or("");
        assert!(vary.contains("Origin"));
    }

    #[actix_web::test]
    async fn no_origin_header_passes_through_cleanly() {
        let reg = Arc::new(InMemoryOriginRegistry::new());
        let cors = DynamicCors::new(platform_config(), reg);
        let app = http_test::init_service(
            App::new().wrap(cors).route("/", web::get().to(ok_handler)),
        )
        .await;

        let req = http_test::TestRequest::get().uri("/").to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(resp.headers().get("access-control-allow-origin").is_none());
        // No Origin header → no Vary either.
        assert!(!resp.headers().contains_key("vary"));
    }

    #[actix_web::test]
    async fn tenant_added_at_runtime_is_allowed_immediately() {
        let (cors, reg) = DynamicCors::with_memory_registry(platform_config());
        let app = http_test::init_service(
            App::new().wrap(cors).route("/", web::get().to(ok_handler)),
        )
        .await;

        // Before registration → rejected.
        let req = http_test::TestRequest::get()
            .uri("/")
            .insert_header(("Origin", "https://login.new-tenant.com"))
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        assert!(resp.headers().get("access-control-allow-origin").is_none());

        // Register the domain at runtime.
        reg.insert("https://login.new-tenant.com");

        // After registration → allowed.
        let req = http_test::TestRequest::get()
            .uri("/")
            .insert_header(("Origin", "https://login.new-tenant.com"))
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(
            resp.headers()
                .get("access-control-allow-origin")
                .and_then(|v| v.to_str().ok()),
            Some("https://login.new-tenant.com")
        );
    }

    #[test]
    fn with_memory_registry_convenience_returns_shared_handle() {
        let (_, reg) = DynamicCors::with_memory_registry(CorsConfig::default());
        reg.insert("https://test.example.com");
        assert_eq!(reg.len(), 1);
    }
}
