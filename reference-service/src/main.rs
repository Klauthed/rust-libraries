//! A small **reference service** wiring the klauthed crates together end to end:
//! profile-aware configuration ([`klauthed_core`]), telemetry init
//! ([`klauthed_observability`]), the actix-web layer with health probes and
//! uniform error rendering ([`klauthed_web`]), JWT issue/verify
//! ([`klauthed_security`]), and a background-jobs pipeline
//! ([`klauthed_platform`]): a [`JobQueue`] fed by HTTP handlers, drained by a
//! [`JobWorker`] that the [`Scheduler`] runs, which delivers user
//! [`Notification`]s.
//!
//! Endpoints:
//! * `GET  /health`, `GET /health/ready` — liveness / readiness (framework).
//! * `POST /login` — issue an HS256 JWT for a (demo) username and enqueue a
//!   welcome notification for the background worker to deliver.
//! * `GET  /api/me` — JWT-protected; echoes the authenticated subject.
//!
//! Run it with `cargo run -p reference-service`. It is a starting template, not a
//! product: the JWT secret is hard-coded here but in a real service comes from
//! configuration / Vault; the in-memory `JobQueue` and `RecordingNotifier` would
//! be a Postgres/Redis queue and a real email/SMS provider — the wiring is
//! identical.

use std::sync::Arc;

use actix_web::{HttpResponse, web};
use klauthed_core::config::{ConfigBuilder, Profile};
use klauthed_core::time::{Duration, SystemClock};
use klauthed_observability::TelemetryConfig;
use klauthed_platform::scheduler::{Cron, Scheduler};
use klauthed_platform::{
    EnqueuedJob, InMemoryJobQueue, JobHandler, JobQueue, JobWorker, Notification, Notifier,
    RecordingNotifier,
};
use klauthed_security::{Claims, JwtSigner, JwtVerifier};
use klauthed_web::{AppError, AuthenticatedUser, JwtAuth};
use serde::{Deserialize, Serialize};

/// Demo signing key. A real service sources this from config / Vault, never the
/// binary — see the `klauthed-core` config and `vault` feature.
const JWT_SECRET: &[u8] = b"reference-service-demo-secret-change-me";

/// The job kind enqueued on login and handled by [`WelcomeHandler`].
const WELCOME_JOB: &str = "send_welcome";

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let profile = Profile::detect();

    // Telemetry first, so config loading and binding are traced.
    let telemetry = TelemetryConfig::for_profile(&profile, "reference-service");
    let _telemetry = klauthed_observability::init(&telemetry).expect("telemetry init");

    let config = ConfigBuilder::new(profile).build().await.expect("configuration");
    let server = config.server().unwrap_or_default();

    // Background-jobs pipeline: an in-memory queue (fed by `/login`), a worker that
    // drains it by delivering a welcome notification, and the platform scheduler
    // driving the worker on an interval. Swap the queue/notifier for durable +
    // real-provider implementations and the rest is unchanged.
    let queue = Arc::new(InMemoryJobQueue::new(Arc::new(SystemClock)));
    let notifier = Arc::new(RecordingNotifier::new());
    let worker = Arc::new(JobWorker::new(
        queue.clone(),
        Arc::new(WelcomeHandler { notifier: Arc::clone(&notifier) }),
    ));

    // The handle is held for the server's lifetime; dropping it (or
    // `shutdown().await`) stops the tasks. A panic in one run is isolated and the
    // schedule continues.
    let _scheduler = Scheduler::new()
        .every(std::time::Duration::from_secs(5), move || {
            let worker = Arc::clone(&worker);
            async move {
                match worker.run_once().await {
                    Ok(processed) if processed > 0 => {
                        tracing::info!(processed, "background worker drained jobs");
                    }
                    Ok(_) => {}
                    Err(error) => tracing::warn!(%error, "background worker error"),
                }
            }
        })
        .cron(Cron::parse("0 * * * *").expect("valid cron"), || async {
            tracing::info!("hourly maintenance tick");
        })
        .start();

    let queue = web::Data::from(queue);
    tracing::info!(bind = %server.bind_address(), "reference-service starting");
    klauthed_web::server::serve_with_defaults(&server, configure(queue))?.await
}

/// Build the app-config closure (registering JWT signer/verifier, the job queue,
/// and routes) shared by `main` (via `serve_with_defaults`) and the tests.
fn configure(queue: web::Data<InMemoryJobQueue>) -> impl Fn(&mut web::ServiceConfig) + Clone {
    move |cfg: &mut web::ServiceConfig| {
        cfg.app_data(web::Data::new(JwtVerifier::hs256(JWT_SECRET)));
        cfg.app_data(web::Data::new(JwtSigner::hs256(JWT_SECRET)));
        cfg.app_data(queue.clone());
        cfg.route("/login", web::post().to(login));
        // Everything under /api requires a valid Bearer token.
        cfg.service(web::scope("/api").wrap(JwtAuth::new()).route("/me", web::get().to(me)));
    }
}

/// Delivers a welcome [`Notification`] for each [`WELCOME_JOB`]. In production
/// the notifier is a real email/SMS provider; here it records what would be sent.
struct WelcomeHandler {
    notifier: Arc<RecordingNotifier>,
}

#[async_trait::async_trait]
impl JobHandler for WelcomeHandler {
    async fn handle(&self, job: &EnqueuedJob) -> Result<(), String> {
        let username =
            job.payload().get("username").and_then(serde_json::Value::as_str).unwrap_or("user");
        self.notifier
            .send(&Notification::email(
                format!("{username}@example.com"),
                "Welcome",
                format!("Welcome aboard, {username}!"),
            ))
            .await
            .map_err(|error| error.to_string())?;
        tracing::info!(%username, "sent welcome notification");
        Ok(())
    }
}

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
}

#[derive(Serialize, Deserialize)]
struct LoginResponse {
    token: String,
}

/// Issue a 1-hour HS256 token for the supplied username (demo: no password check)
/// and enqueue a welcome notification for the background worker to deliver.
async fn login(
    body: web::Json<LoginRequest>,
    signer: web::Data<JwtSigner>,
    queue: web::Data<InMemoryJobQueue>,
) -> Result<HttpResponse, AppError> {
    let claims = Claims::builder(body.username.as_str(), &SystemClock, Duration::hours(1))
        .issuer("reference-service")
        .build();
    let token = signer.encode(&claims)?;
    // Hand the welcome notification off to the background worker.
    queue.enqueue(WELCOME_JOB.into(), serde_json::json!({ "username": body.username })).await?;
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

    /// A fresh in-memory queue wrapped for app data.
    fn test_queue() -> web::Data<InMemoryJobQueue> {
        web::Data::new(InMemoryJobQueue::new(Arc::new(SystemClock)))
    }

    #[actix_web::test]
    async fn health_is_up() {
        let app = http_test::init_service(
            App::new()
                .configure(klauthed_web::health::configure)
                .configure(configure(test_queue())),
        )
        .await;
        let req = http_test::TestRequest::get().uri("/health").to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[actix_web::test]
    async fn protected_route_requires_a_token() {
        let app = http_test::init_service(App::new().configure(configure(test_queue()))).await;
        let req = http_test::TestRequest::get().uri("/api/me").to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn login_then_call_protected_route() {
        let app = http_test::init_service(App::new().configure(configure(test_queue()))).await;

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

    #[actix_web::test]
    async fn login_enqueues_a_welcome_job_the_worker_delivers() {
        // Share one queue between the HTTP app and the worker.
        let queue = test_queue();
        let notifier = Arc::new(RecordingNotifier::new());
        let worker = JobWorker::new(
            queue.clone().into_inner(),
            Arc::new(WelcomeHandler { notifier: Arc::clone(&notifier) }),
        );

        let app = http_test::init_service(App::new().configure(configure(queue.clone()))).await;
        let req = http_test::TestRequest::post()
            .uri("/login")
            .set_json(serde_json::json!({ "username": "alice" }))
            .to_request();
        let _: LoginResponse = http_test::call_and_read_body_json(&app, req).await;

        // The worker drains the queued job and the notifier records the welcome.
        assert_eq!(worker.run_once().await.unwrap(), 1);
        let sent = notifier.sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].recipient, "alice@example.com");
        assert_eq!(sent[0].subject.as_deref(), Some("Welcome"));
    }
}
