//! A minimal actix-web service with klauthed health endpoints.
//!
//! Run with: `cargo run -p klauthed-web --example server`
//! then `curl localhost:8080/health` and `curl localhost:8080/health/ready`.

use actix_web::{App, HttpServer, web};
use klauthed_web::health::{HealthRegistry, configure as configure_health};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    println!("listening on http://127.0.0.1:8080 (Ctrl-C to stop)");

    HttpServer::new(|| {
        App::new()
            // Register infra health checks via `HealthRegistry::with_check(...)`.
            .app_data(web::Data::new(HealthRegistry::new()))
            .configure(configure_health)
            .route("/", web::get().to(|| async { "klauthed service" }))
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
