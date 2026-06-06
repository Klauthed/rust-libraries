//! [`AppError`] — the aggregate error HTTP handlers return.
//!
//! Any [`DomainError`] (a `ConfigError`, `DataError`, or a future crate's error)
//! converts into `AppError`, which captures the error's `category`, stable
//! `code`, and message at construction. Its actix `ResponseError` impl then
//! renders a uniform JSON body and the right HTTP status for every error in the
//! system.
//!
//! Security note: client errors (`4xx`) include their message in the response,
//! but server errors (`5xx`) return a generic message to the client while the
//! real one is logged — so internal details never leak to callers.

use std::error::Error as StdError;
use std::fmt;

use actix_web::http::StatusCode;
use actix_web::{HttpResponse, ResponseError};
use klauthed_error::{DomainError, ErrorCategory, ErrorCode};
use serde::Serialize;

/// Result alias for handlers: `AppResult<T> = Result<T, AppError>`.
pub type AppResult<T> = Result<T, AppError>;

/// The top-level error type for the HTTP layer.
///
/// Build it from any [`DomainError`] (via `?`/`From` for the common crates, or
/// [`AppError::from_domain`] for anything else), or directly for ad-hoc handler
/// failures with the category constructors ([`AppError::bad_request`], …).
pub struct AppError {
    category: ErrorCategory,
    code: ErrorCode,
    message: String,
    source: Option<Box<dyn StdError + Send + Sync + 'static>>,
}

impl AppError {
    /// Build from any [`DomainError`], capturing its category, code, and message
    /// and retaining it as the error [`source`](StdError::source).
    pub fn from_domain<E>(error: E) -> Self
    where
        E: DomainError + Send + Sync + 'static,
    {
        AppError {
            category: error.category(),
            code: error.code(),
            message: error.to_string(),
            source: Some(Box::new(error)),
        }
    }

    /// An ad-hoc error with an explicit category and message and the category's
    /// default code (override with [`with_code`](Self::with_code)).
    pub fn new(category: ErrorCategory, message: impl Into<String>) -> Self {
        AppError { category, code: default_code(category), message: message.into(), source: None }
    }

    /// Override the stable error code.
    pub fn with_code(mut self, code: impl Into<ErrorCode>) -> Self {
        self.code = code.into();
        self
    }

    /// `400 Bad Request`.
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::BadRequest, message)
    }
    /// `401 Unauthorized`.
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Unauthorized, message)
    }
    /// `403 Forbidden`.
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Forbidden, message)
    }
    /// `404 Not Found`.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::NotFound, message)
    }
    /// `422 Unprocessable Entity` — the request was understood but failed
    /// semantic/business validation.
    pub fn unprocessable_entity(message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::UnprocessableEntity, message)
    }
    /// `409 Conflict`.
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Conflict, message)
    }
    /// `429 Too Many Requests` (rate limited).
    pub fn too_many_requests(message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::RateLimited, message)
    }
    /// `500 Internal Server Error`.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Internal, message)
    }

    /// The error's category.
    pub fn category(&self) -> ErrorCategory {
        self.category
    }

    /// The error's stable code.
    pub fn code(&self) -> &ErrorCode {
        &self.code
    }
}

/// The default code for an ad-hoc error of each category.
fn default_code(category: ErrorCategory) -> ErrorCode {
    ErrorCode::new(match category {
        ErrorCategory::BadRequest => "request.bad_request",
        ErrorCategory::Unauthorized => "request.unauthorized",
        ErrorCategory::Forbidden => "request.forbidden",
        ErrorCategory::NotFound => "request.not_found",
        ErrorCategory::UnprocessableEntity => "request.unprocessable_entity",
        ErrorCategory::Conflict => "request.conflict",
        ErrorCategory::RateLimited => "request.rate_limited",
        ErrorCategory::Timeout => "request.timeout",
        ErrorCategory::Unavailable => "request.unavailable",
        ErrorCategory::Internal => "request.internal",
    })
}

// ── From the common domain errors, for `?` ergonomics ─────────────────────────
//
// (No blanket `impl<E: DomainError> From<E>`, which would collide with the std
// reflexive `From<T> for T`; concrete impls are unambiguous.)

impl From<klauthed_core::validation::ValidationErrors> for AppError {
    fn from(error: klauthed_core::validation::ValidationErrors) -> Self {
        Self::from_domain(error)
    }
}

impl From<klauthed_core::error::ConfigError> for AppError {
    fn from(error: klauthed_core::error::ConfigError) -> Self {
        Self::from_domain(error)
    }
}

impl From<klauthed_data::DataError> for AppError {
    fn from(error: klauthed_data::DataError) -> Self {
        Self::from_domain(error)
    }
}

impl From<klauthed_security::SecurityError> for AppError {
    fn from(error: klauthed_security::SecurityError) -> Self {
        Self::from_domain(error)
    }
}

impl From<klauthed_platform::PlatformError> for AppError {
    fn from(error: klauthed_platform::PlatformError) -> Self {
        Self::from_domain(error)
    }
}

impl fmt::Debug for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppError")
            .field("category", &self.category)
            .field("code", &self.code)
            .field("message", &self.message)
            .field("source", &self.source)
            .finish()
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl StdError for AppError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.source.as_deref().map(|s| s as &(dyn StdError + 'static))
    }
}

impl DomainError for AppError {
    fn category(&self) -> ErrorCategory {
        self.category
    }

    fn code(&self) -> ErrorCode {
        self.code.clone()
    }
}

// ── HTTP rendering ────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct ErrorBody<'a> {
    error: ErrorDetail<'a>,
}

#[derive(Serialize)]
struct ErrorDetail<'a> {
    code: &'a str,
    category: &'a str,
    message: String,
}

impl ResponseError for AppError {
    fn status_code(&self) -> StatusCode {
        StatusCode::from_u16(self.category.http_status())
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
    }

    fn error_response(&self) -> HttpResponse {
        let client_facing = self.category.is_client_error();

        if client_facing {
            tracing::debug!(code = %self.code, category = %self.category, "request rejected: {}", self.message);
        } else {
            tracing::error!(code = %self.code, category = %self.category, "request failed: {}", self.message);
        }

        // Never leak server-side detail to the client; the real message is logged above.
        let message =
            if client_facing { self.message.clone() } else { "internal server error".to_owned() };

        HttpResponse::build(self.status_code()).json(ErrorBody {
            error: ErrorDetail {
                code: self.code.as_str(),
                category: self.category.as_str(),
                message,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::body::to_bytes;

    #[test]
    fn status_maps_from_category() {
        assert_eq!(AppError::not_found("x").status_code(), StatusCode::NOT_FOUND);
        assert_eq!(AppError::bad_request("x").status_code().as_u16(), 400);
        assert_eq!(AppError::internal("x").status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn from_config_error_preserves_code_and_category() {
        let err: AppError =
            klauthed_core::error::ConfigError::MissingRequired("database".into()).into();
        assert_eq!(err.code().as_str(), "config.missing_required");
        assert_eq!(err.category(), ErrorCategory::Internal);
        assert!(err.source().is_some());
    }

    #[test]
    fn from_data_error_is_unavailable_for_unsupported() {
        let err: AppError =
            klauthed_data::DataError::UnsupportedSystem(klauthed_core::config::DbSystem::MongoDb)
                .into();
        assert_eq!(err.code().as_str(), "data.unsupported_system");
    }

    #[actix_web::test]
    async fn client_error_body_includes_message() {
        let resp = AppError::bad_request("field 'name' is required").error_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let bytes = to_bytes(resp.into_body()).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["error"]["code"], "request.bad_request");
        assert_eq!(body["error"]["category"], "bad_request");
        assert_eq!(body["error"]["message"], "field 'name' is required");
    }

    #[test]
    fn from_security_error_preserves_code_and_category() {
        let err: AppError = klauthed_security::SecurityError::ExpiredToken.into();
        assert_eq!(err.code().as_str(), "security.expired_token");
        assert_eq!(err.category(), ErrorCategory::Unauthorized);
        assert_eq!(err.status_code().as_u16(), 401);
        assert!(err.source().is_some());
    }

    #[test]
    fn from_security_invalid_token_is_bad_request() {
        let err: AppError =
            klauthed_security::SecurityError::MalformedToken("bad jwt".into()).into();
        assert_eq!(err.code().as_str(), "security.malformed_token");
        assert_eq!(err.category(), ErrorCategory::BadRequest);
    }

    #[test]
    fn from_platform_error_preserves_code_and_category() {
        let err: AppError =
            klauthed_platform::PlatformError::TenantNotFound { id_or_slug: "acme".into() }.into();
        assert_eq!(err.code().as_str(), "platform.tenant_not_found");
        assert_eq!(err.category(), ErrorCategory::NotFound);
        assert_eq!(err.status_code().as_u16(), 404);
        assert!(err.source().is_some());
    }

    #[test]
    fn from_platform_backend_is_internal() {
        let err: AppError =
            klauthed_platform::PlatformError::Backend { message: "db conn failed".into() }.into();
        assert_eq!(err.category(), ErrorCategory::Internal);
    }

    #[test]
    fn unprocessable_entity_constructor_gives_422() {
        let err = AppError::unprocessable_entity("email must be a valid address");
        assert_eq!(err.status_code().as_u16(), 422);
        assert_eq!(err.category(), ErrorCategory::UnprocessableEntity);
        assert_eq!(err.code().as_str(), "request.unprocessable_entity");
    }

    #[test]
    fn validation_errors_surface_as_422() {
        use klauthed_core::validation::{Validate, ValidationErrors};
        struct Bad;
        impl Validate for Bad {
            fn validate(&self) -> Result<(), ValidationErrors> {
                let mut e = ValidationErrors::new();
                e.add("name", "required", "name is required");
                e.into_result()
            }
        }
        let app_err: AppError = Bad.validate().unwrap_err().into();
        assert_eq!(app_err.status_code().as_u16(), 422);
        assert_eq!(app_err.code().as_str(), "validation.failed");
    }

    #[actix_web::test]
    async fn server_error_body_hides_message_but_keeps_code() {
        // A sensitive internal message must not reach the client.
        let resp = AppError::internal("db password = hunter2").error_response();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let bytes = to_bytes(resp.into_body()).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["error"]["message"], "internal server error");
        assert_eq!(body["error"]["code"], "request.internal");
        assert_eq!(body["error"]["category"], "internal");
    }
}
