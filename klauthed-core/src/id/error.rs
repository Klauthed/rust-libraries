//! The [`ParseIdError`] returned when a string is neither a UUID nor a ULID.

use std::fmt;

use klauthed_error::{DomainError, ErrorCategory, ErrorCode};

/// Error returned when a string is neither a valid UUID nor ULID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseIdError {
    input: String,
}

impl ParseIdError {
    pub(crate) fn new(input: &str) -> Self {
        Self { input: input.to_owned() }
    }
}

impl fmt::Display for ParseIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "'{}' is not a valid UUID or ULID", self.input)
    }
}

impl std::error::Error for ParseIdError {}

impl DomainError for ParseIdError {
    fn category(&self) -> ErrorCategory {
        ErrorCategory::BadRequest
    }

    fn code(&self) -> ErrorCode {
        ErrorCode::new("id.invalid")
    }
}
