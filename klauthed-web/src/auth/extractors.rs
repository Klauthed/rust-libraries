//! Request extractors: [`AuthenticatedUser`] (requires authentication) and
//! [`OptionalAuthentication`] (never fails).

use std::future::{ready, Ready};
use std::ops::Deref;

use actix_web::{Error, FromRequest, HttpMessage as _, HttpRequest};
use klauthed_security::Claims;

use crate::error::AppError;

// ── Extractors ────────────────────────────────────────────────────────────────

/// Extractor that provides the [`Claims`] of an authenticated request.
///
/// Returns `401 Unauthorized` if no claims are present in the request
/// extensions. Claims are populated by [`JwtAuth`](super::JwtAuth); without it this extractor
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
