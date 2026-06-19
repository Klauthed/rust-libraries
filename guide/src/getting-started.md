# Getting started

## Scaffold a new service

The quickest start is the scaffolding CLI, installed as a cargo subcommand:

```sh
cargo install klauthed-cli
cargo klauthed new my-service
cd my-service && cargo run
```

This generates a ready-to-run service — `Cargo.toml`, `src/main.rs`,
`config/default.toml`, a README, and tests — that already serves `/hello` plus
the framework's `/health` and `/health/ready` probes. Add `--with-jwt` to instead
scaffold a `/login` endpoint and a JWT-protected `/api/me` route. Grow it by
enabling more `klauthed` features (below). The rest of this page shows what that
wiring looks like by hand.

## Add the dependency

Depend on the umbrella crate and enable the pieces you need as cargo features:

```toml
[dependencies]
klauthed = { version = "0.6", features = ["web", "data", "security", "observability"] }
```

Each top-level feature re-exports the matching crate as a module
(`klauthed::web`, `klauthed::data`, …) and pulls in its dependencies. Prefer a
single crate? Depend on it directly — e.g. `klauthed-core` — they all publish
independently.

> The minimum supported Rust version is **1.95**, edition **2024**.

## A minimal service

```rust,ignore
use klauthed::core::config::{ConfigBuilder, Profile};
use klauthed::web::{health, AppError};
use actix_web::{App, HttpServer, web, HttpResponse};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // 1. Load layered, profile-aware configuration once at startup.
    let profile = Profile::detect();
    let config = ConfigBuilder::new(profile).build().await.expect("config");
    let server = config.server().expect("server config");

    // 2. Build the app: your routes + the framework's health probes.
    HttpServer::new(|| {
        App::new()
            .configure(health::configure)
            .route("/hello", web::get().to(|| async { HttpResponse::Ok().body("hi") }))
    })
    .bind((server.host.as_str(), server.port))?
    .run()
    .await
}
```

## Feature flags, briefly

klauthed is feature-gated to the bone so a service compiles only what it uses.
A few you will reach for early:

| Feature (on `klauthed`) | Turns on |
|-------------------------|----------|
| `web` | the actix-web layer (`klauthed-web`) |
| `data` + `postgres` / `redis` / … | connection pools and data patterns |
| `security` | JWT/PASETO, AEAD, password hashing, … |
| `observability` + `otel` / `metrics` | tracing, OpenTelemetry, Prometheus |
| `openapi` / `swagger-ui` | generated OpenAPI spec + bundled UI |
| `config-server` | run the service *as* a config server |

The next chapters walk through the [architecture](architecture.md), the
[configuration](configuration.md) model, and a
[capability map](capabilities.md) of every crate.
