//! OpenAPI 3.1 spec generation (`openapi` feature).
//!
//! Annotate handlers with [`utoipa::path`], derive [`utoipa::OpenApi`] for your
//! service, merge in the built-in [`base_openapi`] document (the health probes),
//! and serve it as JSON with [`serve_spec`]. Point any OpenAPI viewer (Swagger
//! UI, Redoc, Scalar, …) at that URL.
//!
//! ```no_run
//! use actix_web::{App, HttpServer};
//! use klauthed_web::openapi;
//!
//! # async fn run() -> std::io::Result<()> {
//! HttpServer::new(|| {
//!     App::new().configure(|cfg| {
//!         openapi::serve_spec(cfg, "/api-docs/openapi.json", openapi::base_openapi());
//!     })
//! })
//! .bind(("0.0.0.0", 8080))?
//! .run()
//! .await
//! # }
//! ```

use actix_web::{HttpResponse, web};
use utoipa::OpenApi;

/// Re-export of [`utoipa`] so services annotate their handlers and derive their
/// document against the exact version klauthed-web was built with.
pub use utoipa;

/// The OpenAPI document for klauthed-web's built-in endpoints (currently the
/// liveness/readiness health probes).
///
/// Merge it into your service's own document with
/// [`utoipa::openapi::OpenApi::merge`] (or `nest`) so the generated spec covers
/// both your routes and the framework's.
#[must_use]
pub fn base_openapi() -> utoipa::openapi::OpenApi {
    crate::health::routes::HealthApi::openapi()
}

/// Mount `GET {path}` to serve `doc` as the OpenAPI JSON document.
pub fn serve_spec(cfg: &mut web::ServiceConfig, path: &str, doc: utoipa::openapi::OpenApi) {
    cfg.app_data(web::Data::new(doc));
    cfg.route(path, web::get().to(spec_handler));
}

async fn spec_handler(doc: web::Data<utoipa::openapi::OpenApi>) -> HttpResponse {
    HttpResponse::Ok().json(doc.get_ref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{App, test as http_test};
    use serde_json::Value;

    #[actix_web::test]
    async fn serves_the_built_in_openapi_document() {
        let app = http_test::init_service(App::new().configure(|cfg| {
            serve_spec(cfg, "/api-docs/openapi.json", base_openapi());
        }))
        .await;

        let req = http_test::TestRequest::get().uri("/api-docs/openapi.json").to_request();
        let doc: Value = http_test::call_and_read_body_json(&app, req).await;

        // A valid OpenAPI 3.x document that documents the health probes.
        assert!(doc.get("openapi").and_then(Value::as_str).is_some_and(|v| v.starts_with("3.")));
        assert!(doc.pointer("/paths/~1health").is_some(), "missing /health path: {doc}");
        assert!(doc.pointer("/paths/~1health~1ready").is_some(), "missing /health/ready path");
    }
}
