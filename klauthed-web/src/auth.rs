//! JWT Bearer token authentication middleware and extractors.
//!
//! Three cooperating pieces:
//!
//! * [`JwtAuth`] — an actix [`Transform`] that enforces a valid
//!   `Authorization: Bearer <token>` header on every request in its scope. The
//!   decoded [`Claims`] are stored in request extensions so extractors can read
//!   them. Requests with a missing, malformed, or expired token are rejected
//!   with `401 Unauthorized` before reaching any handler.
//! * [`AuthenticatedUser`] — a [`FromRequest`] extractor that yields the
//!   [`Claims`] from the current request. Returns `401` if no claims are
//!   present (i.e. the middleware was not mounted or the route is outside its
//!   scope). Derefs to [`Claims`] so all accessors are available directly.
//! * [`OptionalAuthentication`] — a [`FromRequest`] extractor that yields
//!   `Option<Claims>`, never failing. Useful for routes that serve both
//!   authenticated and anonymous callers.
//!
//! # Wiring
//!
//! 1. Build your [`JwtVerifier`] (once, shared across workers) and register it
//!    as [`web::Data`]:
//!
//! ```no_run
//! use actix_web::{web, App};
//! use klauthed_security::JwtVerifier;
//! use klauthed_web::auth::JwtAuth;
//!
//! let verifier = JwtVerifier::hs256(b"my-signing-secret");
//! let _app = App::new()
//!     .app_data(web::Data::new(verifier))
//!     .wrap(JwtAuth::new());
//! ```
//!
//! 2. Require auth in handlers by accepting [`AuthenticatedUser`]:
//!
//! ```no_run
//! use actix_web::HttpResponse;
//! use klauthed_web::auth::AuthenticatedUser;
//!
//! async fn protected(user: AuthenticatedUser) -> HttpResponse {
//!     let subject = user.sub().unwrap_or("unknown");
//!     HttpResponse::Ok().body(format!("hello, {subject}"))
//! }
//! ```
//!
//! # Security notes
//!
//! * The `Authorization` prefix must be exactly `"Bearer "` (case-sensitive,
//!   one trailing space). Other schemes (Basic, Digest, …) are rejected with
//!   `401`.
//! * Token expiry (`exp`) and not-before (`nbf`) are enforced by the
//!   [`JwtVerifier`]. Configure [`leeway_seconds`] on the verifier if your
//!   upstream clocks skew.
//! * `5xx` server error messages are never included in responses (actix's
//!   `ResponseError` rendering via [`AppError`] handles this). Configuration
//!   errors (e.g. a missing verifier) produce `500` without leaking details.
//!
//! [`leeway_seconds`]: klauthed_security::JwtVerifier::leeway_seconds

use std::future::{ready, Ready};
use std::ops::Deref;
use std::rc::Rc;
use std::task::{Context as TaskContext, Poll};

use std::sync::Arc;

use actix_web::body::BoxBody;
use actix_web::dev::{Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::http::header::AUTHORIZATION;
use actix_web::{Error, FromRequest, HttpMessage as _, HttpRequest, ResponseError as _, web};
use klauthed_core::context::RequestContext;
use futures_util::future::LocalBoxFuture;
use klauthed_error::DomainError as _;
use klauthed_security::{Claims, JwtVerifier, SecurityError, TokenDenylist};

use crate::error::AppError;

// ── TokenRevocationCheck ──────────────────────────────────────────────────────

/// A concrete wrapper around `Arc<dyn TokenDenylist>` for actix app data.
///
/// Register this with `App::app_data(web::Data::new(TokenRevocationCheck(denylist)))`.
/// When present, [`JwtAuth`] will check every token's `jti` against the
/// denylist before admitting the request.
///
/// ```no_run
/// use std::sync::Arc;
/// use actix_web::{web, App};
/// use klauthed_security::InMemoryTokenDenylist;
/// use klauthed_web::auth::{JwtAuth, TokenRevocationCheck};
///
/// let denylist = Arc::new(InMemoryTokenDenylist::new());
/// let _app = App::new()
///     .app_data(web::Data::new(TokenRevocationCheck(denylist)))
///     .wrap(JwtAuth::new());
/// ```
pub struct TokenRevocationCheck(pub Arc<dyn TokenDenylist>);

/// `"Bearer "` — the mandatory prefix for the `Authorization` header value.
const BEARER_PREFIX: &str = "Bearer ";

/// Extract `<token>` from an `Authorization: Bearer <token>` header on a
/// `ServiceRequest`, or `None` when the header is absent or uses a different
/// scheme.
fn extract_bearer_token(req: &ServiceRequest) -> Option<String> {
    let value = req.headers().get(AUTHORIZATION)?;
    let s = value.to_str().ok()?;
    s.strip_prefix(BEARER_PREFIX).map(str::to_owned)
}

// ── JwtAuth middleware ────────────────────────────────────────────────────────

/// Actix [`Transform`] that validates JWT Bearer tokens.
///
/// Wrap a scope or the whole app with `.wrap(JwtAuth::new())` and register the
/// configured [`JwtVerifier`] as `web::Data<JwtVerifier>`. Every request that
/// passes through will have its `Authorization: Bearer` token decoded and the
/// resulting [`Claims`] stored in the request extensions.
///
/// Requests without a Bearer token, or with an invalid / expired token, are
/// rejected with `401 Unauthorized` before any handler runs.
///
/// # Panics
///
/// Does not panic; a missing `web::Data<JwtVerifier>` returns `500` with a
/// logged error so misconfiguration surfaces clearly in development.
#[derive(Debug, Clone, Default)]
pub struct JwtAuth {
    _priv: (),
}

impl JwtAuth {
    /// Construct the middleware.
    pub fn new() -> Self {
        Self::default()
    }
}

impl<S, B> Transform<S, ServiceRequest> for JwtAuth
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: actix_web::body::MessageBody + 'static,
{
    type Response = ServiceResponse<BoxBody>;
    type Error = Error;
    type Transform = JwtAuthService<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(JwtAuthService {
            service: Rc::new(service),
        }))
    }
}

/// The [`Service`] produced by [`JwtAuth`].
pub struct JwtAuthService<S> {
    service: Rc<S>,
}

/// Reject a request with `err`, boxing the error response body.
fn reject(req: ServiceRequest, err: AppError) -> ServiceResponse<BoxBody> {
    req.into_response(err.error_response().map_into_boxed_body())
}

impl<S, B> Service<ServiceRequest> for JwtAuthService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: actix_web::body::MessageBody + 'static,
{
    type Response = ServiceResponse<BoxBody>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&self, cx: &mut TaskContext<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&self, req: ServiceRequest) -> Self::Future {
        // A missing JwtVerifier is a server configuration error, not a caller
        // mistake — log it loudly and surface 500 without leaking details.
        let Some(verifier) = req.app_data::<web::Data<JwtVerifier>>().cloned() else {
            tracing::error!(
                "JwtAuth: no JwtVerifier registered as web::Data — \
                 did you forget App::app_data(web::Data::new(verifier))?"
            );
            return Box::pin(ready(Ok(reject(
                req,
                AppError::internal("authentication is not configured"),
            ))));
        };

        // A missing Authorization header or wrong scheme → 401 immediately.
        let Some(token) = extract_bearer_token(&req) else {
            return Box::pin(ready(Ok(reject(
                req,
                AppError::unauthorized("authentication required"),
            ))));
        };

        // Validate the token; map SecurityError to a client-safe 401/400.
        let claims = match verifier.decode(&token) {
            Ok(c) => c,
            Err(e) => {
                // Log the technical reason server-side; send a minimal message to
                // the client so validation internals are never exposed.
                let client_msg = match &e {
                    SecurityError::ExpiredToken => "token has expired",
                    SecurityError::MalformedToken(_) => "token is malformed",
                    _ => "token is invalid",
                };
                tracing::debug!(error = %e, "jwt bearer validation failed");
                let app_err = AppError::new(e.category(), client_msg).with_code(e.code());
                return Box::pin(ready(Ok(reject(req, app_err))));
            }
        };

        // Happy path: propagate the authenticated subject into RequestContext so
        // audit events, observability spans, and structured logs all carry the
        // principal without the handler having to thread it through manually.
        if let Some(sub) = claims.sub.as_deref() {
            let updated_ctx = req
                .extensions()
                .get::<RequestContext>()
                .cloned()
                .unwrap_or_default()
                .with_principal(sub);
            req.extensions_mut().insert(updated_ctx);
        }

        // Extract jti before moving claims into extensions (needed for revocation
        // check inside the async block).
        let jti = claims.jti.clone();

        // Store decoded claims for the AuthenticatedUser / OptionalAuthentication
        // extractors — separate from RequestContext so both are independently
        // accessible.
        req.extensions_mut().insert(claims);

        // Optional: check jti against a registered TokenDenylist.
        // If web::Data<TokenRevocationCheck> is present AND the token carries
        // a jti, we check whether it has been revoked.
        let denylist = req
            .app_data::<web::Data<TokenRevocationCheck>>()
            .map(|d| Arc::clone(&d.0));

        let service = Rc::clone(&self.service);
        Box::pin(async move {
            if let (Some(ref dl), Some(ref jti)) = (denylist, jti) {
                match dl.is_revoked(jti).await {
                    Ok(true) => {
                        return Ok(reject(req, AppError::unauthorized("token has been revoked")));
                    }
                    Ok(false) => {}
                    Err(e) => {
                        tracing::error!(error = %e, "token denylist check failed");
                        return Ok(reject(
                            req,
                            AppError::internal("authentication check failed"),
                        ));
                    }
                }
            }
            service
                .call(req)
                .await
                .map(ServiceResponse::map_into_boxed_body)
        })
    }
}

// ── Extractors ────────────────────────────────────────────────────────────────

/// Extractor that provides the [`Claims`] of an authenticated request.
///
/// Returns `401 Unauthorized` if no claims are present in the request
/// extensions. Claims are populated by [`JwtAuth`]; without it this extractor
/// always returns `401`.
///
/// `Deref<Target = Claims>` gives direct access to `sub`, `iss`, `aud`,
/// `custom`, etc.
///
/// ```no_run
/// use actix_web::HttpResponse;
/// use klauthed_web::auth::AuthenticatedUser;
///
/// async fn handler(user: AuthenticatedUser) -> HttpResponse {
///     let subject = user.sub().unwrap_or("unknown");
///     HttpResponse::Ok().body(format!("hello, {subject}"))
/// }
/// ```
#[derive(Debug, Clone)]
pub struct AuthenticatedUser(Claims);

impl AuthenticatedUser {
    /// The underlying [`Claims`].
    pub fn claims(&self) -> &Claims {
        &self.0
    }

    /// Consume the extractor and return the owned [`Claims`].
    pub fn into_claims(self) -> Claims {
        self.0
    }

    /// The token's `sub` claim (the authenticated principal's id), if present.
    pub fn sub(&self) -> Option<&str> {
        self.0.sub.as_deref()
    }
}

impl AuthenticatedUser {
    /// Parse and return the scopes from the `scope` claim (space-separated).
    ///
    /// Returns an empty `Vec` if the claim is absent or is not a string. The
    /// `scope` claim follows [RFC 6749 §3.3] convention: a space-delimited list
    /// of case-sensitive strings.
    ///
    /// [RFC 6749 §3.3]: https://www.rfc-editor.org/rfc/rfc6749#section-3.3
    pub fn scopes(&self) -> Vec<&str> {
        self.0
            .custom
            .get("scope")
            .and_then(|v| v.as_str())
            .map(|s| s.split_whitespace().collect())
            .unwrap_or_default()
    }

    /// Return `true` if **all** `required` scopes are present in the token's
    /// `scope` claim.
    pub fn has_scopes(&self, required: &[&str]) -> bool {
        let token_scopes = self.scopes();
        required.iter().all(|r| token_scopes.contains(r))
    }

    /// Return `true` if the single `scope` is present in the token's
    /// `scope` claim.
    pub fn has_scope(&self, scope: &str) -> bool {
        self.has_scopes(&[scope])
    }

    /// Require `scope` to be present, or return `AppError::forbidden(...)`.
    ///
    /// Ergonomic inline scope enforcement inside handlers:
    ///
    /// ```ignore
    /// async fn handler(user: AuthenticatedUser) -> AppResult<HttpResponse> {
    ///     user.require_scope("admin:write")?;
    ///     Ok(HttpResponse::Ok().finish())
    /// }
    /// ```
    pub fn require_scope(&self, scope: &str) -> crate::error::AppResult<()> {
        self.require_scopes(&[scope])
    }

    /// Require **all** `required` scopes to be present, or return
    /// `AppError::forbidden(...)`.
    pub fn require_scopes(&self, required: &[&str]) -> crate::error::AppResult<()> {
        if self.has_scopes(required) {
            Ok(())
        } else {
            Err(crate::error::AppError::forbidden(format!(
                "token is missing required scope(s): {}",
                required.join(", ")
            )))
        }
    }
}

impl Deref for AuthenticatedUser {
    type Target = Claims;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<AuthenticatedUser> for Claims {
    fn from(u: AuthenticatedUser) -> Self {
        u.0
    }
}

impl FromRequest for AuthenticatedUser {
    type Error = Error;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _: &mut actix_web::dev::Payload) -> Self::Future {
        match req.extensions().get::<Claims>().cloned() {
            Some(claims) => ready(Ok(AuthenticatedUser(claims))),
            None => ready(Err(AppError::unauthorized("authentication required").into())),
        }
    }
}

/// Extractor that provides `Option<Claims>` — never fails.
///
/// Returns `Some(claims)` when the request has been authenticated (claims are
/// in the extensions), `None` otherwise. Useful for routes that serve both
/// authenticated and anonymous callers.
///
/// ```no_run
/// use actix_web::HttpResponse;
/// use klauthed_web::auth::OptionalAuthentication;
///
/// async fn handler(auth: OptionalAuthentication) -> HttpResponse {
///     match auth.into_inner() {
///         Some(claims) => HttpResponse::Ok().body(format!("hello, {:?}", claims.sub)),
///         None => HttpResponse::Ok().body("hello, stranger"),
///     }
/// }
/// ```
#[derive(Debug, Clone)]
pub struct OptionalAuthentication(Option<Claims>);

impl OptionalAuthentication {
    /// The [`Claims`] if the request is authenticated, or `None`.
    pub fn claims(&self) -> Option<&Claims> {
        self.0.as_ref()
    }

    /// Consume and return the inner `Option<Claims>`.
    pub fn into_inner(self) -> Option<Claims> {
        self.0
    }

    /// Whether the request carries validated claims.
    pub fn is_authenticated(&self) -> bool {
        self.0.is_some()
    }
}

impl FromRequest for OptionalAuthentication {
    type Error = Error;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _: &mut actix_web::dev::Payload) -> Self::Future {
        let claims = req.extensions().get::<Claims>().cloned();
        ready(Ok(OptionalAuthentication(claims)))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::http::StatusCode;
    use actix_web::test as http_test;
    use actix_web::{web, App, HttpResponse};
    use klauthed_core::time::SystemClock;
    use klauthed_security::jwt::{Claims, JwtSigner};
    use klauthed_security::JwtVerifier;

    const SECRET: &[u8] = b"test-signing-secret";

    fn signer() -> JwtSigner {
        JwtSigner::hs256(SECRET)
    }

    fn verifier() -> JwtVerifier {
        JwtVerifier::hs256(SECRET)
    }

    /// Mint a fresh, valid HS256 token for `subject`.
    fn valid_token(subject: &str) -> String {
        signer()
            .encode(
                &Claims::builder(subject, &SystemClock, chrono::Duration::hours(1)).build(),
            )
            .unwrap()
    }

    /// Mint a token whose `exp` is already in the past.
    fn expired_token() -> String {
        signer()
            .encode(
                &Claims::builder("u", &SystemClock, chrono::Duration::hours(-1)).build(),
            )
            .unwrap()
    }

    async fn echo_sub(user: AuthenticatedUser) -> HttpResponse {
        HttpResponse::Ok().body(user.sub().unwrap_or("").to_owned())
    }

    async fn echo_optional(auth: OptionalAuthentication) -> HttpResponse {
        match auth.into_inner() {
            Some(c) => HttpResponse::Ok().body(c.sub.unwrap_or_default()),
            None => HttpResponse::Ok().body("anonymous"),
        }
    }

    macro_rules! auth_app {
        () => {
            http_test::init_service(
                App::new()
                    .app_data(web::Data::new(verifier()))
                    .wrap(JwtAuth::new())
                    .route("/protected", web::get().to(echo_sub))
                    .route("/optional", web::get().to(echo_optional)),
            )
            .await
        };
    }

    // Handler that reads RequestContext.principal() to verify propagation.
    async fn echo_principal(ctx: crate::context::Context) -> HttpResponse {
        HttpResponse::Ok().body(ctx.principal().unwrap_or("none").to_owned())
    }

    #[actix_web::test]
    async fn jwt_auth_propagates_sub_into_request_context() {
        let app = http_test::init_service(
            App::new()
                .app_data(web::Data::new(verifier()))
                .wrap(JwtAuth::new())
                .wrap(crate::context::RequestContextMiddleware::new())
                .route("/principal", web::get().to(echo_principal)),
        )
        .await;

        let token = valid_token("alice");
        let req = http_test::TestRequest::get()
            .uri("/principal")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();

        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        // RequestContext.principal() must equal the JWT sub claim.
        let body = http_test::read_body(resp).await;
        assert_eq!(&body[..], b"alice");
    }

    #[actix_web::test]
    async fn valid_token_reaches_handler_with_claims() {
        let app = auth_app!();
        let token = valid_token("alice");
        let req = http_test::TestRequest::get()
            .uri("/protected")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body = http_test::read_body(resp).await;
        assert_eq!(&body[..], b"alice");
    }

    #[actix_web::test]
    async fn missing_authorization_header_returns_401() {
        let app = auth_app!();
        let req = http_test::TestRequest::get().uri("/protected").to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn non_bearer_scheme_returns_401() {
        let app = auth_app!();
        let req = http_test::TestRequest::get()
            .uri("/protected")
            .insert_header(("Authorization", "Basic dXNlcjpwYXNz"))
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn expired_token_returns_401() {
        let app = auth_app!();
        let token = expired_token();
        let req = http_test::TestRequest::get()
            .uri("/protected")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let body = http_test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["code"], "security.expired_token");
    }

    #[actix_web::test]
    async fn wrong_secret_returns_401() {
        let app = auth_app!();
        let token = JwtSigner::hs256(b"wrong-secret")
            .encode(
                &Claims::builder("eve", &SystemClock, chrono::Duration::hours(1)).build(),
            )
            .unwrap();
        let req = http_test::TestRequest::get()
            .uri("/protected")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let body = http_test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["code"], "security.invalid_token");
    }

    #[actix_web::test]
    async fn malformed_token_returns_400() {
        let app = auth_app!();
        let req = http_test::TestRequest::get()
            .uri("/protected")
            .insert_header(("Authorization", "Bearer not.a.jwt"))
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        // MalformedToken → BadRequest (400).
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[actix_web::test]
    async fn optional_authentication_no_token_rejected_by_middleware() {
        // JwtAuth wraps the whole app, so a missing token is rejected before
        // the OptionalAuthentication extractor even runs.
        let app = auth_app!();
        let req = http_test::TestRequest::get().uri("/optional").to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    /// Test the OptionalAuthentication extractor in isolation (without JwtAuth).
    #[actix_web::test]
    async fn optional_extractor_without_middleware_returns_none() {
        let app = http_test::init_service(
            App::new().route("/optional", web::get().to(echo_optional)),
        )
        .await;

        let req = http_test::TestRequest::get().uri("/optional").to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body = http_test::read_body(resp).await;
        assert_eq!(&body[..], b"anonymous");
    }

    /// Test the OptionalAuthentication extractor when JwtAuth ran successfully.
    #[actix_web::test]
    async fn optional_extractor_with_middleware_returns_some() {
        let app = http_test::init_service(
            App::new()
                .app_data(web::Data::new(verifier()))
                .wrap(JwtAuth::new())
                .route("/optional", web::get().to(echo_optional)),
        )
        .await;

        let token = valid_token("bob");
        let req = http_test::TestRequest::get()
            .uri("/optional")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();

        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body = http_test::read_body(resp).await;
        assert_eq!(&body[..], b"bob");
    }

    #[actix_web::test]
    async fn missing_verifier_returns_500() {
        // No web::Data<JwtVerifier> registered → configuration error → 500.
        let app = http_test::init_service(
            App::new()
                .wrap(JwtAuth::new())
                .route("/", web::get().to(|| async { HttpResponse::Ok().finish() })),
        )
        .await;

        let req = http_test::TestRequest::get()
            .uri("/")
            .insert_header(("Authorization", format!("Bearer {}", valid_token("u"))))
            .to_request();

        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[actix_web::test]
    async fn authenticated_user_extractor_fails_without_claims_in_extensions() {
        // No JwtAuth middleware → no claims in extensions → extractor returns Err.
        let app = http_test::init_service(
            App::new().route("/", web::get().to(echo_sub)),
        )
        .await;

        let req = http_test::TestRequest::get().uri("/").to_request();
        let resp = http_test::call_service(&app, req).await;
        // Handler requires AuthenticatedUser; no claims → 401.
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn revoked_token_jti_returns_401() {
        use std::sync::Arc;
        use klauthed_security::{InMemoryTokenDenylist, TokenDenylist as _};

        let denylist = Arc::new(InMemoryTokenDenylist::new());

        let jti = "unique-jti-abc123";
        let token = JwtSigner::hs256(SECRET)
            .encode(
                &Claims::builder("alice", &SystemClock, chrono::Duration::hours(1))
                    .jwt_id(jti)
                    .build(),
            )
            .unwrap();

        // Use a concrete far-future timestamp that's within chrono's valid range
        // (i64::MAX / 2 overflows chrono and falls back to now, immediately evicting
        // the entry). Year 2099 ≈ 4102444800000 ms, well within range.
        let far_future = klauthed_core::time::Timestamp::now()
            .checked_add(chrono::Duration::days(365 * 10))
            .unwrap();
        denylist.revoke(jti.into(), far_future).await.unwrap();

        let app = http_test::init_service(
            App::new()
                .app_data(web::Data::new(verifier()))
                .app_data(web::Data::new(TokenRevocationCheck(
                    denylist as Arc<dyn klauthed_security::TokenDenylist>,
                )))
                .wrap(JwtAuth::new())
                .route("/", web::get().to(echo_sub)),
        )
        .await;

        let req = http_test::TestRequest::get()
            .uri("/")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn non_revoked_token_passes_denylist_check() {
        use std::sync::Arc;
        use klauthed_security::InMemoryTokenDenylist;

        let token = JwtSigner::hs256(SECRET)
            .encode(
                &Claims::builder("alice", &SystemClock, chrono::Duration::hours(1))
                    .jwt_id("not-revoked")
                    .build(),
            )
            .unwrap();

        let app = http_test::init_service(
            App::new()
                .app_data(web::Data::new(verifier()))
                .app_data(web::Data::new(TokenRevocationCheck(
                    Arc::new(InMemoryTokenDenylist::new()),
                )))
                .wrap(JwtAuth::new())
                .route("/", web::get().to(echo_sub)),
        )
        .await;

        let req = http_test::TestRequest::get()
            .uri("/")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
