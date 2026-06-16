//! Run this service **as** a config server (`feature = "config-server"`).
//!
//! Turns a klauthed service into the configuration server other services pull
//! from — a Rust-native alternative to Spring Cloud Config Server. Mount a
//! [`ConfigServer`] and it answers `GET /{application}/{profile}[/{label}]` with
//! the merged configuration tree, read from a [`ConfigSource`] (a directory of
//! TOML/JSON files, or in-memory).
//!
//! It is the server counterpart to
//! [`ConfigServerProvider`](klauthed_core::config::provider::ConfigServerProvider):
//! a client points its provider at this server (with the `Klauthed` format) and
//! its config is served from here.
//!
//! ```no_run
//! use actix_web::{App, HttpServer};
//! use klauthed_web::config_server::{ConfigServer, DirectoryConfigSource};
//!
//! # async fn run() -> std::io::Result<()> {
//! let server = ConfigServer::new(DirectoryConfigSource::new("config-repo"));
//! HttpServer::new(move || App::new().configure(|cfg| server.configure(cfg)))
//!     .bind(("0.0.0.0", 8888))?
//!     .run()
//!     .await
//! # }
//! ```

mod source;

use std::sync::Arc;

use actix_web::{HttpResponse, web};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use source::{ConfigSource, ConfigSourceError, DirectoryConfigSource, InMemoryConfigSource};

/// The klauthed-native config response: the resolved configuration tree plus the
/// coordinates it was resolved for.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigDocument {
    /// The application the configuration is for.
    pub application: String,
    /// The profile.
    pub profile: String,
    /// The label (e.g. a git ref), when one was requested.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The merged configuration tree.
    pub config: Value,
}

/// A mountable config server backed by a [`ConfigSource`].
///
/// Cheap to clone (the source is shared); build it once and let actix clone it
/// per worker.
#[derive(Clone)]
pub struct ConfigServer {
    source: Arc<dyn ConfigSource>,
}

impl ConfigServer {
    /// Build a config server over `source`.
    #[must_use]
    pub fn new(source: impl ConfigSource) -> Self {
        Self { source: Arc::new(source) }
    }

    /// Build from an already-shared source.
    #[must_use]
    pub fn from_arc(source: Arc<dyn ConfigSource>) -> Self {
        Self { source }
    }

    /// Mount the source and the config-server routes onto an actix app/scope:
    /// `GET /{application}/{profile}` and `GET /{application}/{profile}/{label}`.
    ///
    /// Mount under a [`scope`](actix_web::web::scope) (e.g. `/config`) to embed
    /// the server alongside other routes.
    pub fn configure(&self, cfg: &mut web::ServiceConfig) {
        cfg.app_data(web::Data::new(Arc::clone(&self.source)));
        cfg.route("/{application}/{profile}", web::get().to(serve));
        cfg.route("/{application}/{profile}/{label}", web::get().to(serve_labeled));
    }
}

async fn serve(
    path: web::Path<(String, String)>,
    source: web::Data<Arc<dyn ConfigSource>>,
) -> HttpResponse {
    let (application, profile) = path.into_inner();
    respond(&source, application, profile, None).await
}

async fn serve_labeled(
    path: web::Path<(String, String, String)>,
    source: web::Data<Arc<dyn ConfigSource>>,
) -> HttpResponse {
    let (application, profile, label) = path.into_inner();
    respond(&source, application, profile, Some(label)).await
}

async fn respond(
    source: &Arc<dyn ConfigSource>,
    application: String,
    profile: String,
    label: Option<String>,
) -> HttpResponse {
    match source.fetch(&application, &profile, label.as_deref()).await {
        Ok(map) => {
            let config = serde_json::to_value(map.into_inner())
                .unwrap_or_else(|_| Value::Object(Default::default()));
            HttpResponse::Ok().json(ConfigDocument { application, profile, label, config })
        }
        Err(error) => {
            tracing::error!(%error, %application, %profile, "config source failed");
            HttpResponse::InternalServerError().finish()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{App, test as http_test};
    use klauthed_core::config::ConfigMap;
    use serde_json::json;

    #[actix_web::test]
    async fn serves_the_native_config_document() {
        let source = InMemoryConfigSource::new().with(
            "auth-api",
            "prod",
            ConfigMap::from_iter([("port".to_owned(), json!(8443))]).expand_dotted(),
        );
        let server = ConfigServer::new(source);

        let app = http_test::init_service(App::new().configure(|cfg| server.configure(cfg))).await;

        let req = http_test::TestRequest::get().uri("/auth-api/prod").to_request();
        let doc: ConfigDocument = http_test::call_and_read_body_json(&app, req).await;

        assert_eq!(doc.application, "auth-api");
        assert_eq!(doc.profile, "prod");
        assert_eq!(doc.config.get("port"), Some(&json!(8443)));
    }

    #[actix_web::test]
    async fn unknown_application_serves_empty_config() {
        let server = ConfigServer::new(InMemoryConfigSource::new());
        let app = http_test::init_service(App::new().configure(|cfg| server.configure(cfg))).await;

        let req = http_test::TestRequest::get().uri("/nope/dev").to_request();
        let doc: ConfigDocument = http_test::call_and_read_body_json(&app, req).await;

        assert_eq!(doc.config, json!({}));
    }
}
