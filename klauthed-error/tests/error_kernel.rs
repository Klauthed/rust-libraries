//! Integration tests for the error kernel.
//!
//! These exercise only the public API — exactly what a downstream crate sees
//! when it implements [`DomainError`] for its own error type.

use klauthed_error::{DomainError, ErrorCategory, ErrorCode};

/// A representative per-crate error type implementing the kernel contract.
#[derive(Debug)]
enum WidgetError {
    NotFound,
    UpstreamDown,
}

impl std::fmt::Display for WidgetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WidgetError::NotFound => f.write_str("widget not found"),
            WidgetError::UpstreamDown => f.write_str("widget service is unavailable"),
        }
    }
}

impl std::error::Error for WidgetError {}

impl DomainError for WidgetError {
    fn category(&self) -> ErrorCategory {
        match self {
            WidgetError::NotFound => ErrorCategory::NotFound,
            WidgetError::UpstreamDown => ErrorCategory::Unavailable,
        }
    }

    fn code(&self) -> ErrorCode {
        match self {
            WidgetError::NotFound => ErrorCode::new("widget.not_found"),
            WidgetError::UpstreamDown => ErrorCode::new("widget.upstream_down"),
        }
    }
}

#[test]
fn category_drives_http_status_and_retry_defaults() {
    let not_found = WidgetError::NotFound;
    assert_eq!(not_found.category(), ErrorCategory::NotFound);
    assert_eq!(not_found.http_status(), 404);
    assert!(!not_found.is_retryable());
    assert_eq!(not_found.code().as_str(), "widget.not_found");

    // `Unavailable` is transient, so the default retry policy says "retry".
    let down = WidgetError::UpstreamDown;
    assert_eq!(down.http_status(), 503);
    assert!(down.is_retryable());
}

#[test]
fn category_classification_helpers() {
    assert!(ErrorCategory::BadRequest.is_client_error());
    assert!(!ErrorCategory::Internal.is_client_error());
    assert_eq!(ErrorCategory::RateLimited.http_status(), 429);
    assert!(ErrorCategory::RateLimited.is_retryable());
    assert_eq!(ErrorCategory::UnprocessableEntity.as_str(), "unprocessable_entity");
}
