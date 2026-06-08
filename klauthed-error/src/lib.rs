#![deny(unsafe_code)]
#![deny(missing_docs)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)
)]

//! The klauthed error **kernel**.
//!
//! This crate deliberately holds no error *types* — those live with the domains
//! that raise them (`ConfigError` in `klauthed-core`, `DataError` in
//! `klauthed-data`, …). Instead it defines the shared *contract* every klauthed
//! error implements, so the whole system classifies, codes, and surfaces errors
//! the same way:
//!
//! * [`ErrorCategory`] — coarse classification driving HTTP status / retryability.
//! * [`ErrorCode`] — a stable `domain.reason` code for logs and API responses.
//! * [`DomainError`] — the trait each concrete error type implements.
//!
//! It has zero required dependencies (enable the `serde` feature to serialize
//! codes/categories), so everything can depend on it without pulling weight and
//! without creating cycles: types stay home, only the contract is shared.
//!
//! ```
//! use klauthed_error::{DomainError, ErrorCategory, ErrorCode};
//!
//! #[derive(Debug)]
//! struct WidgetMissing(String);
//! impl std::fmt::Display for WidgetMissing {
//!     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//!         write!(f, "widget {} not found", self.0)
//!     }
//! }
//! impl std::error::Error for WidgetMissing {}
//!
//! impl DomainError for WidgetMissing {
//!     fn category(&self) -> ErrorCategory { ErrorCategory::NotFound }
//!     fn code(&self) -> ErrorCode { ErrorCode::new("widget.not_found") }
//! }
//!
//! let err = WidgetMissing("w-1".into());
//! assert_eq!(err.http_status(), 404);
//! assert!(!err.is_retryable());
//! ```

mod category;
mod code;

pub use category::ErrorCategory;
pub use code::ErrorCode;

/// The contract every klauthed error type implements.
///
/// Concrete error enums stay in their own crates and implement this to plug into
/// shared handling (HTTP mapping, retry decisions, structured logging). The
/// `category()` answer supplies sensible defaults for [`is_retryable`] and
/// [`http_status`], so most impls only define `category` and `code`.
///
/// [`is_retryable`]: DomainError::is_retryable
/// [`http_status`]: DomainError::http_status
pub trait DomainError: std::error::Error {
    /// The coarse classification of this error.
    fn category(&self) -> ErrorCategory;

    /// The stable `domain.reason` code for this error.
    fn code(&self) -> ErrorCode;

    /// Whether retrying might help. Defaults to the category's policy.
    fn is_retryable(&self) -> bool {
        self.category().is_retryable()
    }

    /// The HTTP status to surface. Defaults to the category's status.
    fn http_status(&self) -> u16 {
        self.category().http_status()
    }
}
