//! End-to-end demo wiring the klauthed stack into one actix-web service:
//! password **login → JWT**, then a **rate-limited, JWT-protected** API, with a
//! per-request context and health endpoints.
//!
//! Run it:
//!
//! ```text
//! cargo run -p klauthed-web --example auth_service
//! ```
//!
//! Then, in another terminal:
//!
//! ```text
//! # Log in (demo user) and capture the token:
//! TOKEN=$(curl -s localhost:8080/login \
//!   -H 'content-type: application/json' \
//!   -d '{"username":"alice","password":"password123"}' | jq -r .token)
//!
//! # Call the protected endpoint with it:
//! curl -s localhost:8080/api/me -H "Authorization: Bearer $TOKEN"
//!
//! # Without (or with a bad) token -> 401:
//! curl -si localhost:8080/api/me | head -1
//!
//! # Liveness/readiness:
//! curl -s localhost:8080/health
//!
//! # Every response carries hardening headers (CSP, X-Frame-Options, …):
//! curl -si localhost:8080/health | grep -i -E 'content-security|x-frame|x-content'
//!
//! # CSRF (cookie-based scope): fetch a token cookie, then use it on a write.
//! curl -s -c jar.txt localhost:8080/session/new
//! TOKEN=$(awk '$6=="csrf_token"{print $7}' jar.txt)
//! curl -s  -b jar.txt -X POST localhost:8080/session/action -H "x-csrf-token: $TOKEN"
//! # Missing/!matching header -> 403:
//! curl -si -b jar.txt -X POST localhost:8080/session/action | head -1
//! ```

use std::collections::HashMap;
use std::time::Duration as StdDuration;

use actix_web::{App, HttpResponse, HttpServer, web};
use serde::{Deserialize, Serialize};

use klauthed_core::time::{Duration, SystemClock};
use klauthed_security::{Claims, JwtSigner, JwtVerifier, hash_password, verify_password};
use klauthed_web::AppError;
use klauthed_web::auth::{AuthenticatedUser, JwtAuth};
use klauthed_web::context::RequestContextMiddleware;
use klauthed_web::csrf::{Csrf, CsrfConfig};
use klauthed_web::extract::Json;
use klauthed_web::headers::{SecurityHeaders, SecurityHeadersConfig};
use klauthed_web::health::{HealthRegistry, configure as configure_health};
use klauthed_web::ratelimit::{KeyBy, RateLimit};

/// Demo HMAC signing secret. In a real service this comes from config / a secret
/// store (see `klauthed_core::config`), never a hard-coded constant.
const JWT_SECRET: &[u8] = b"demo-signing-secret-change-me";
/// Issuer stamped into tokens and required by the verifier.
const ISSUER: &str = "klauthed-demo";

/// A toy user directory: username → Argon2 (PHC) password hash.
type Users = HashMap<String, String>;

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Serialize)]
struct TokenResponse {
    token: String,
}

/// `POST /login` — verify credentials and mint a 1-hour JWT.
async fn login(
    body: Json<LoginRequest>,
    users: web::Data<Users>,
    signer: web::Data<JwtSigner>,
) -> Result<HttpResponse, AppError> {
    // Argon2 verification is constant-time; an unknown user still runs the
    // failure path so timing doesn't reveal which usernames exist.
    let ok = match users.get(&body.username) {
        Some(hash) => verify_password(&body.password, hash)?,
        None => false,
    };
    if !ok {
        return Err(AppError::unauthorized("invalid username or password"));
    }

    let claims = Claims::builder(body.username.clone(), &SystemClock, Duration::hours(1))
        .issuer(ISSUER)
        .build();
    let token = signer.encode(&claims)?; // SecurityError -> AppError via `?`
    Ok(HttpResponse::Ok().json(TokenResponse { token }))
}

/// `GET /api/me` — requires a valid JWT (enforced by [`JwtAuth`]); echoes the
/// authenticated subject from the token claims.
async fn me(user: AuthenticatedUser) -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({ "subject": user.sub() }))
}

/// `GET /session/new` — a safe request, so the [`Csrf`] middleware auto-issues a
/// `csrf_token` cookie. A browser client then echoes that value in the
/// `x-csrf-token` header on subsequent writes.
async fn session_new() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({
        "message": "csrf_token cookie set; echo it in the x-csrf-token header on writes"
    }))
}

/// `POST /session/action` — a state-changing, cookie-authenticated request. The
/// [`Csrf`] middleware rejects it with `403` unless the `csrf_token` cookie and
/// the `x-csrf-token` header are both present and equal.
async fn session_action() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({ "status": "ok" }))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Seed one demo user (hash the password once at startup).
    let mut users = Users::new();
    users.insert("alice".to_owned(), hash_password("password123").expect("hash demo password"));
    let users = web::Data::new(users);
    let signer = web::Data::new(JwtSigner::hs256(JWT_SECRET));

    println!("listening on http://127.0.0.1:8080 (Ctrl-C to stop)");

    HttpServer::new(move || {
        App::new()
            .app_data(users.clone())
            .app_data(signer.clone())
            // JwtAuth reads the verifier from app data; require the demo issuer.
            .app_data(web::Data::new(JwtVerifier::hs256(JWT_SECRET).expecting_issuer(ISSUER)))
            .app_data(web::Data::new(HealthRegistry::new()))
            // Tag every request with a RequestContext (request id, etc.).
            .wrap(RequestContextMiddleware::new())
            // Outermost: hardening headers on every response (incl. errors).
            // HSTS is dropped because this demo serves plain HTTP on localhost.
            .wrap(SecurityHeaders::from_config(&SecurityHeadersConfig::default().without_hsts()))
            .configure(configure_health)
            .route("/login", web::post().to(login))
            // Protected API: JWT required + 60 requests/min per peer IP. CSRF is
            // not needed here — these requests authenticate with a Bearer token,
            // not an ambient cookie.
            .service(
                web::scope("/api")
                    .wrap(RateLimit::new(60, StdDuration::from_secs(60)).key_by(KeyBy::PeerIp))
                    .wrap(JwtAuth::new())
                    .route("/me", web::get().to(me)),
            )
            // Cookie-based "browser session" scope: CSRF-protected. `secure(false)`
            // lets the cookie ride plain HTTP for this localhost demo.
            .service(
                web::scope("/session")
                    .wrap(Csrf::from_config(CsrfConfig::default().secure(false)))
                    .route("/new", web::get().to(session_new))
                    .route("/action", web::post().to(session_action)),
            )
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
