//! Error type for the testing utilities (`TestingError`).

use klauthed_macros::DomainError;

/// Errors raised by the testing utilities themselves.
///
/// These are rare — most helpers are infallible — but a few operations (e.g.
/// constructing fixtures from malformed input) can fail, and they report through
/// the shared [`DomainError`](klauthed_error::DomainError) contract like every
/// other klauthed error.
#[derive(Debug, DomainError)]
#[domain(prefix = "testing", category = "internal")]
#[non_exhaustive]
pub enum TestingError {
    /// A fixture could not be built from the provided input.
    Fixture(String),
}

impl std::fmt::Display for TestingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TestingError::Fixture(msg) => write!(f, "failed to build fixture: {msg}"),
        }
    }
}

impl std::error::Error for TestingError {}

#[cfg(test)]
mod tests {
    use super::*;
    use klauthed_error::{DomainError, ErrorCategory};

    #[test]
    fn fixture_error_is_internal() {
        let err = TestingError::Fixture("nope".into());
        assert_eq!(err.category(), ErrorCategory::Internal);
        assert_eq!(err.code().as_str(), "testing.fixture");
        assert!(err.to_string().contains("nope"));
    }
}
