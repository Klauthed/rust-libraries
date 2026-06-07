//! The [`AppError`] type: construction, category/code/message, and its
//! std error-trait impls.

use std::error::Error as StdError;
use std::fmt;

use klauthed_error::{DomainError, ErrorCategory, ErrorCode};

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

impl AppError {
    /// The raw (server-side) message. Crate-internal: the HTTP renderer in
    /// [`response`](super::response) uses it to decide what to surface.
    pub(crate) fn message(&self) -> &str {
        &self.message
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
