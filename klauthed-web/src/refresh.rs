//! Config push-refresh endpoint (`config-refresh` feature).
//!
//! [`serve_refresh`] mounts an HTTP route that pokes a
//! [`RefreshTrigger`](klauthed_core::config::RefreshTrigger), so an operator or a
//! config-bus webhook can force a live configuration reload — the Spring Cloud
//! `/actuator/refresh` analog. Pair it with
//! [`ReloadableConfig::start_with_refresh`](klauthed_core::config::ReloadableConfig::start_with_refresh).
//!
//! The route is unauthenticated by default; mount it under an authenticated
//! scope (or behind `JwtAuth`) since forcing reloads is a privileged action.
//!
//! ```no_run
//! use actix_web::{App, HttpServer};
//! use klauthed_core::config::RefreshTrigger;
//! use klauthed_web::refresh;
//!
//! # async fn run(trigger: RefreshTrigger) -> std::io::Result<()> {
//! HttpServer::new(move || {
//!     let trigger = trigger.clone();
//!     App::new().configure(move |cfg| refresh::serve_refresh(cfg, "/refresh", trigger.clone()))
//! })
//! .bind(("0.0.0.0", 8080))?
//! .run()
//! .await
//! # }
//! ```

use actix_web::{HttpResponse, web};
use klauthed_core::config::RefreshTrigger;

/// Mount `POST {path}` to trigger a configuration reload via `trigger`.
///
/// Responds `202 Accepted`: the reload runs asynchronously on the
/// `ReloadableConfig` task and is coalesced, so rapid calls collapse into one.
pub fn serve_refresh(cfg: &mut web::ServiceConfig, path: &str, trigger: RefreshTrigger) {
    cfg.app_data(web::Data::new(trigger));
    cfg.route(path, web::post().to(refresh_handler));
}

async fn refresh_handler(trigger: web::Data<RefreshTrigger>) -> HttpResponse {
    trigger.refresh();
    HttpResponse::Accepted().finish()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use actix_web::http::StatusCode;
    use actix_web::{App, test as http_test};
    use klauthed_core::config::{ConfigBuilder, Profile, ReloadableConfig};

    #[actix_web::test]
    async fn post_refresh_returns_202_and_rejects_other_methods() {
        // A real trigger from a ReloadableConfig (long interval ⇒ only this
        // endpoint would drive a reload).
        let builder = ConfigBuilder::new(Profile::Test);
        let (config, trigger) =
            ReloadableConfig::start_with_refresh(builder, Duration::from_secs(3600)).await.unwrap();

        let app = http_test::init_service(
            App::new().configure(|cfg| serve_refresh(cfg, "/refresh", trigger)),
        )
        .await;

        let req = http_test::TestRequest::post().uri("/refresh").to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::ACCEPTED);

        // The route is POST-only: a GET is rejected (404/405, never the handler).
        let req = http_test::TestRequest::get().uri("/refresh").to_request();
        let resp = http_test::call_service(&app, req).await;
        assert!(resp.status().is_client_error(), "GET should be rejected, got {}", resp.status());

        drop(config); // keep the reloadable config alive until here
    }
}
