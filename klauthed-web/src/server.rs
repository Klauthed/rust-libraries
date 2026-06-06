//! HTTP server bootstrap from a [`ServerConfig`].
//!
//! actix's [`HttpServer::new`] takes an *app factory* (a closure run once per
//! worker thread), so this module gives the caller two entry points that drive
//! that factory while wiring in the workspace conventions from
//! [`ServerConfig`]:
//!
//! * [`serve`] — bind the caller's [`App`] factory verbatim, applying
//!   `workers` and `bind_address()` from config.
//! * [`serve_with_defaults`] — convenience wrapper that additionally wraps every
//!   built `App` with [`RequestContextMiddleware`] and mounts the health
//!   endpoints via [`health::configure`], so a service only has to supply its
//!   own routes/data.
//!
//! Both return a bound [`actix_web::dev::Server`]; the caller decides whether to
//! `.await` it (run) or drop it (e.g. in tests that only assert it binds).
//!
//! ```no_run
//! use actix_web::{web, HttpResponse};
//! use klauthed_core::config::ServerConfig;
//! use klauthed_web::server;
//!
//! # async fn run() -> std::io::Result<()> {
//! let config = ServerConfig::default();
//! let server = server::serve_with_defaults(&config, |cfg: &mut web::ServiceConfig| {
//!     cfg.route("/hello", web::get().to(|| async { HttpResponse::Ok().finish() }));
//! })?;
//! server.await
//! # }
//! ```
//!
//! # Out of scope (future passes)
//!
//! TLS termination (`ServerConfig::tls`), graceful-shutdown tuning, and a full
//! CLI runner are intentionally not handled here yet; `tls = true` is accepted
//! but not acted upon.

use actix_web::body::MessageBody;
use actix_web::dev::{Server, ServiceFactory, ServiceRequest, ServiceResponse};
use actix_web::{web, App, Error, HttpServer};
use klauthed_core::config::ServerConfig;

use crate::app::Components;
use crate::context::RequestContextMiddleware;
use crate::health;

/// Build and bind an [`HttpServer`] from `config`, using `factory` to construct
/// each worker's [`App`].
///
/// Applies `config.workers` (when set) and binds to `config.bind_address()`.
/// The returned [`Server`] is bound but not yet running — `.await` it to run.
///
/// TLS is not yet handled; if `config.tls` is set a warning is logged and the
/// server still binds plaintext.
pub fn serve<F, I, B>(config: &ServerConfig, factory: F) -> std::io::Result<Server>
where
    F: Fn() -> App<I> + Send + Clone + 'static,
    I: ServiceFactory<
            ServiceRequest,
            Config = (),
            Response = ServiceResponse<B>,
            Error = Error,
            InitError = (),
        > + 'static,
    B: MessageBody + 'static,
{
    if config.tls {
        tracing::warn!(
            "ServerConfig.tls is set but TLS termination is not yet supported; binding plaintext"
        );
    }

    let mut server = HttpServer::new(factory);
    if let Some(workers) = config.workers {
        server = server.workers(workers);
    }
    Ok(server.bind(config.bind_address())?.run())
}

/// Like [`serve`], but wraps each built [`App`] with
/// [`RequestContextMiddleware`] and mounts the health endpoints, so the caller's
/// `factory` only needs to add its own routes and app data.
///
/// The supplied factory returns the *configuration* of an app (routes, data,
/// further middleware); this function turns that into a fully wired worker app.
pub fn serve_with_defaults<F>(config: &ServerConfig, factory: F) -> std::io::Result<Server>
where
    F: Fn(&mut web::ServiceConfig) + Send + Clone + 'static,
{
    serve(config, move || {
        let user = factory.clone();
        App::new()
            .wrap(RequestContextMiddleware::new())
            .configure(health::configure)
            .configure(user)
    })
}

/// Like [`serve_with_defaults`], but additionally wires every component from
/// `components` as [`web::Data`] and registers their health checks into the
/// readiness probe — without any manual [`HealthRegistry`] or
/// `SqlHealthCheck` boilerplate.
///
/// This is the entry point for the Spring Boot Actuator-style zero-config
/// health experience: add infra to [`Components`], supply your routes, ship.
///
/// [`HealthRegistry`]: crate::health::HealthRegistry
///
/// ```no_run
/// use klauthed_web::{app::Components, server};
/// use klauthed_core::config::ServerConfig;
///
/// # async fn run() -> std::io::Result<()> {
/// let config = ServerConfig::default();
/// let components = Components::new(); // .pool("db", pool) etc.
///
/// server::serve_with_components(&config, components, |cfg| {
///     // actix_web::web::ServiceConfig — your routes only
/// })?
/// .await
/// # }
/// ```
pub fn serve_with_components<F>(
    config: &ServerConfig,
    components: Components,
    factory: F,
) -> std::io::Result<Server>
where
    F: Fn(&mut web::ServiceConfig) + Send + Clone + 'static,
{
    serve(config, move || {
        let comps = components.clone();
        let user = factory.clone();
        App::new()
            .wrap(RequestContextMiddleware::new())
            .configure(health::configure)
            .configure(move |cfg| comps.configure(cfg))
            .configure(user)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{web, HttpResponse};

    fn ephemeral_config() -> ServerConfig {
        ServerConfig {
            host: "127.0.0.1".to_owned(),
            port: 0, // ephemeral
            workers: Some(1),
            ..ServerConfig::default()
        }
    }

    #[actix_web::test]
    async fn serve_binds_to_ephemeral_port() {
        let config = ephemeral_config();
        let server = serve(&config, || {
            App::new().route(
                "/",
                web::get().to(|| async { HttpResponse::Ok().finish() }),
            )
        })
        .expect("server should bind to 127.0.0.1:0");
        // Binding succeeded; drop without awaiting so we don't block forever.
        drop(server);
    }

    #[actix_web::test]
    async fn serve_with_defaults_binds_and_wires_health() {
        let config = ephemeral_config();
        let server = serve_with_defaults(&config, |cfg: &mut web::ServiceConfig| {
            cfg.route(
                "/ping",
                web::get().to(|| async { HttpResponse::Ok().finish() }),
            );
        })
        .expect("server with defaults should bind");
        drop(server);
    }

    #[actix_web::test]
    async fn serve_honors_tls_flag_without_panicking() {
        let mut config = ephemeral_config();
        config.tls = true; // accepted, logs a warning, still binds plaintext
        let server = serve(&config, || {
            App::new().route(
                "/",
                web::get().to(|| async { HttpResponse::Ok().finish() }),
            )
        })
        .expect("binds even with tls flag set");
        drop(server);
    }
}
