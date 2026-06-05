//! Terse assertions for [`DomainError`] values.
//!
//! Error tests otherwise repeat `assert_eq!(err.category(), ...)` and
//! `assert_eq!(err.code().as_str(), ...)`. These helpers — both free functions
//! and a [`DomainErrorExt`] extension trait — make the intent obvious and the
//! panic messages descriptive.
//!
//! ```
//! use klauthed_testing::assertions::{assert_category, assert_code, DomainErrorExt};
//! use klauthed_error::{DomainError, ErrorCategory, ErrorCode};
//!
//! #[derive(Debug)]
//! struct NotThere;
//! impl std::fmt::Display for NotThere {
//!     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//!         f.write_str("missing")
//!     }
//! }
//! impl std::error::Error for NotThere {}
//! impl DomainError for NotThere {
//!     fn category(&self) -> ErrorCategory { ErrorCategory::NotFound }
//!     fn code(&self) -> ErrorCode { ErrorCode::new("thing.not_found") }
//! }
//!
//! let err = NotThere;
//! // Free functions:
//! assert_category(&err, ErrorCategory::NotFound);
//! assert_code(&err, "thing.not_found");
//! // Or the fluent extension trait:
//! err.assert_category(ErrorCategory::NotFound)
//!    .assert_code("thing.not_found")
//!    .assert_http_status(404);
//! ```

use klauthed_error::{DomainError, ErrorCategory};

/// Assert that `err`'s [`category`](DomainError::category) equals `expected`.
///
/// # Panics
/// Panics with the actual category and code if they differ.
#[track_caller]
pub fn assert_category<E: DomainError + ?Sized>(err: &E, expected: ErrorCategory) {
    let actual = err.category();
    assert!(
        actual == expected,
        "expected category {expected:?}, got {actual:?} (code: {}, error: {err})",
        err.code()
    );
}

/// Assert that `err`'s [`code`](DomainError::code) string equals `expected`.
///
/// # Panics
/// Panics with the actual code and category if they differ.
#[track_caller]
pub fn assert_code<E: DomainError + ?Sized>(err: &E, expected: &str) {
    let actual = err.code();
    assert!(
        actual.as_str() == expected,
        "expected code '{expected}', got '{actual}' (category: {:?}, error: {err})",
        err.category()
    );
}

/// Assert that `err`'s [`http_status`](DomainError::http_status) equals `expected`.
///
/// # Panics
/// Panics with the actual status if it differs.
#[track_caller]
pub fn assert_http_status<E: DomainError + ?Sized>(err: &E, expected: u16) {
    let actual = err.http_status();
    assert!(
        actual == expected,
        "expected HTTP status {expected}, got {actual} (code: {}, error: {err})",
        err.code()
    );
}

/// Assert that `err`'s [`is_retryable`](DomainError::is_retryable) equals `expected`.
///
/// # Panics
/// Panics if the retryability differs.
#[track_caller]
pub fn assert_retryable<E: DomainError + ?Sized>(err: &E, expected: bool) {
    let actual = err.is_retryable();
    assert!(
        actual == expected,
        "expected is_retryable = {expected}, got {actual} (code: {}, error: {err})",
        err.code()
    );
}

/// Fluent assertions on any [`DomainError`], each returning `&self` so they chain.
pub trait DomainErrorExt: DomainError {
    /// Assert this error's category. Returns `&self` for chaining.
    #[track_caller]
    fn assert_category(&self, expected: ErrorCategory) -> &Self {
        assert_category(self, expected);
        self
    }

    /// Assert this error's code string. Returns `&self` for chaining.
    #[track_caller]
    fn assert_code(&self, expected: &str) -> &Self {
        assert_code(self, expected);
        self
    }

    /// Assert this error's HTTP status. Returns `&self` for chaining.
    #[track_caller]
    fn assert_http_status(&self, expected: u16) -> &Self {
        assert_http_status(self, expected);
        self
    }

    /// Assert this error's retryability. Returns `&self` for chaining.
    #[track_caller]
    fn assert_retryable(&self, expected: bool) -> &Self {
        assert_retryable(self, expected);
        self
    }
}

impl<E: DomainError + ?Sized> DomainErrorExt for E {}

#[cfg(test)]
mod tests {
    use super::*;
    use klauthed_error::ErrorCode;

    #[derive(Debug)]
    struct Sample;
    impl std::fmt::Display for Sample {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("sample error")
        }
    }
    impl std::error::Error for Sample {}
    impl DomainError for Sample {
        fn category(&self) -> ErrorCategory {
            ErrorCategory::Unavailable
        }
        fn code(&self) -> ErrorCode {
            ErrorCode::new("sample.down")
        }
    }

    #[test]
    fn free_functions_pass_on_match() {
        let err = Sample;
        assert_category(&err, ErrorCategory::Unavailable);
        assert_code(&err, "sample.down");
        assert_http_status(&err, 503);
        assert_retryable(&err, true);
    }

    #[test]
    fn extension_trait_chains() {
        Sample
            .assert_category(ErrorCategory::Unavailable)
            .assert_code("sample.down")
            .assert_http_status(503)
            .assert_retryable(true);
    }

    #[test]
    #[should_panic(expected = "expected category")]
    fn category_mismatch_panics() {
        assert_category(&Sample, ErrorCategory::NotFound);
    }

    #[test]
    #[should_panic(expected = "expected code")]
    fn code_mismatch_panics() {
        assert_code(&Sample, "sample.up");
    }
}
