//! A Prometheus `GET /metrics` scrape endpoint.
//!
//! Mount it from the [`MetricsHandle`] that
//! [`klauthed_observability::init`](klauthed_observability) returns (or
//! [`metrics::install`](klauthed_observability::metrics::install)):
//!
//! ```no_run
//! use klauthed_web::metrics;
//! # fn configure(cfg: &mut actix_web::web::ServiceConfig) {
//! # let handle = klauthed_observability::metrics::install().unwrap();
//! metrics::serve_metrics(cfg, handle); // GET /metrics → Prometheus text
//! # }
//! ```

use actix_web::{HttpResponse, web};
use klauthed_observability::metrics::MetricsHandle;

/// Mount `GET /metrics` serving the Prometheus exposition format rendered from
/// `handle`. Add this inside the factory passed to
/// [`server::serve_with_defaults`](crate::server::serve_with_defaults).
pub fn serve_metrics(cfg: &mut web::ServiceConfig, handle: MetricsHandle) {
    cfg.app_data(web::Data::new(handle));
    cfg.route("/metrics", web::get().to(render));
}

async fn render(handle: web::Data<MetricsHandle>) -> HttpResponse {
    HttpResponse::Ok()
        .content_type("text/plain; version=0.0.4; charset=utf-8")
        .body(handle.render())
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::http::StatusCode;
    use actix_web::{App, test};

    #[actix_web::test]
    async fn metrics_endpoint_renders_recorded_metrics() {
        // Install the global recorder once, record a sample, and serve it.
        let handle = klauthed_observability::metrics::install().expect("install recorder");
        klauthed_observability::metrics::inc_counter("klauthed_web_metrics_test_total", 1, &[]);

        let app =
            test::init_service(App::new().configure(move |cfg| serve_metrics(cfg, handle))).await;

        let req = test::TestRequest::get().uri("/metrics").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        assert!(content_type.starts_with("text/plain"), "got content-type {content_type}");

        let body = test::read_body(resp).await;
        let text = String::from_utf8_lossy(&body);
        assert!(
            text.contains("klauthed_web_metrics_test_total"),
            "expected the recorded counter in: {text}"
        );
    }
}
