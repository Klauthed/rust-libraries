//! A small **reference service** wiring the klauthed crates together end to end:
//! profile-aware configuration ([`klauthed_core`]), telemetry init
//! ([`klauthed_observability`]), the actix-web layer with health probes and
//! uniform error rendering ([`klauthed_web`]), JWT issue/verify
//! ([`klauthed_security`]), and a scheduled background task
//! ([`klauthed_platform`]).
//!
//! Endpoints:
//! * `GET  /health`, `GET /health/ready` — liveness / readiness (framework).
//! * `POST /login` — issue an HS256 JWT for a (demo) username.
//! * `GET  /api/me` — JWT-protected; echoes the authenticated subject.
//!
//! Run it with `cargo run -p reference-service`. It is a starting template, not a
//! product: the JWT secret is hard-coded here but in a real service comes from
//! configuration / Vault, and a data layer plugs in via `klauthed-data`.

use actix_web::{HttpResponse, web};
use klauthed_core::config::{ConfigBuilder, Profile};
use klauthed_core::time::{Duration, SystemClock};
use klauthed_observability::TelemetryConfig;
use klauthed_platform::scheduler::{Cron, Scheduler};
use klauthed_security::{Claims, JwtSigner, JwtVerifier};
use klauthed_web::{AppError, AuthenticatedUser, JwtAuth};
use serde::{Deserialize, Serialize};

/// Demo signing key. A real service sources this from config / Vault, never the
/// binary — see the `klauthed-core` config and `vault` feature.
const JWT_SECRET: &[u8] = b"reference-service-demo-secret-change-me";

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let profile = Profile::detect();

    // Telemetry first, so config loading and binding are traced.
    let telemetry = TelemetryConfig::for_profile(&profile, "reference-service");
    let _telemetry = klauthed_observability::init(&telemetry).expect("telemetry init");

    let config = ConfigBuilder::new(profile).build().await.expect("configuration");
    let server = config.server().unwrap_or_default();

    // Recurring background work (klauthed-platform `scheduler`). The handle is
    // held for the server's lifetime; dropping it (or `shutdown().await`) stops
    // the tasks. A panic in one run is isolated and the schedule continues.
    let _scheduler = Scheduler::new()
        .cron(Cron::parse("0 * * * *").expect("valid cron"), || async {
            tracing::info!("hourly maintenance tick");
        })
        .start();

    tracing::info!(bind = %server.bind_address(), "reference-service starting");
    klauthed_web::server::serve_with_defaults(&server, configure)?.await
}

/// Register the service's app data (JWT signer/verifier) and routes. Shared by
/// `main` (via `serve_with_defaults`) and the tests.
fn configure(cfg: &mut web::ServiceConfig) {
    cfg.app_data(web::Data::new(JwtVerifier::hs256(JWT_SECRET)));
    cfg.app_data(web::Data::new(JwtSigner::hs256(JWT_SECRET)));
    cfg.route("/login", web::post().to(login));
    // Everything under /api requires a valid Bearer token.
    cfg.service(web::scope("/api").wrap(JwtAuth::new()).route("/me", web::get().to(me)));
}

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
}

#[derive(Serialize, Deserialize)]
struct LoginResponse {
    token: String,
}

/// Issue a 1-hour HS256 token for the supplied username (demo: no password check).
async fn login(
    body: web::Json<LoginRequest>,
    signer: web::Data<JwtSigner>,
) -> Result<HttpResponse, AppError> {
    let claims = Claims::builder(body.username.as_str(), &SystemClock, Duration::hours(1))
        .issuer("reference-service")
        .build();
    let token = signer.encode(&claims)?;
    Ok(HttpResponse::Ok().json(LoginResponse { token }))
}

#[derive(Serialize, Deserialize)]
struct MeResponse {
    sub: String,
}

/// Return the authenticated subject. `JwtAuth` has already validated the token;
/// [`AuthenticatedUser`] yields its claims.
async fn me(user: AuthenticatedUser) -> Result<HttpResponse, AppError> {
    let sub = user.sub().unwrap_or_default().to_owned();
    Ok(HttpResponse::Ok().json(MeResponse { sub }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::http::StatusCode;
    use actix_web::{App, test as http_test};

    #[actix_web::test]
    async fn health_is_up() {
        let app = http_test::init_service(
            App::new().configure(klauthed_web::health::configure).configure(configure),
        )
        .await;
        let req = http_test::TestRequest::get().uri("/health").to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[actix_web::test]
    async fn protected_route_requires_a_token() {
        let app = http_test::init_service(App::new().configure(configure)).await;
        let req = http_test::TestRequest::get().uri("/api/me").to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn login_then_call_protected_route() {
        let app = http_test::init_service(App::new().configure(configure)).await;

        // Log in → token.
        let req = http_test::TestRequest::post()
            .uri("/login")
            .set_json(serde_json::json!({ "username": "alice" }))
            .to_request();
        let body: LoginResponse = http_test::call_and_read_body_json(&app, req).await;
        assert!(!body.token.is_empty());

        // Use the token on the protected route.
        let req = http_test::TestRequest::get()
            .uri("/api/me")
            .insert_header(("authorization", format!("Bearer {}", body.token)))
            .to_request();
        let me: MeResponse = http_test::call_and_read_body_json(&app, req).await;
        assert_eq!(me.sub, "alice");
    }
}
