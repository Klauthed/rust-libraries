#![deny(unsafe_code)]

//! Lightweight, structured validation.
//!
//! A type implements [`Validate`] to check its own invariants, accumulating
//! every problem into [`ValidationErrors`] (rather than failing on the first).
//! Each [`ValidationError`] carries an optional `field`, a stable `code`, and a
//! human message — so the same data drives both API responses and logs.
//!
//! [`ValidationErrors`] is a [`DomainError`] (category `BadRequest`,
//! code `validation.failed`), so it flows through the shared error handling.
//!
//! ```
//! use klauthed_core::validation::{Validate, ValidationErrors};
//!
//! struct SignUp { email: String, age: u8 }
//!
//! impl Validate for SignUp {
//!     fn validate(&self) -> Result<(), ValidationErrors> {
//!         let mut errors = ValidationErrors::new();
//!         if !self.email.contains('@') {
//!             errors.add("email", "invalid_email", "must be a valid email address");
//!         }
//!         if self.age < 18 {
//!             errors.add("age", "too_small", "must be at least 18");
//!         }
//!         errors.into_result()
//!     }
//! }
//!
//! assert!(SignUp { email: "x".into(), age: 10 }.validate().is_err());
//! assert!(SignUp { email: "a@b.co".into(), age: 21 }.validate().is_ok());
//! ```

use std::borrow::Cow;
use std::fmt;

use klauthed_error::{DomainError, ErrorCategory, ErrorCode};
use serde::{Deserialize, Serialize};

/// A single validation problem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationError {
    /// The field this applies to, or `None` for object-level errors.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    /// A stable, machine-readable code (e.g. `required`, `too_long`).
    pub code: Cow<'static, str>,
    /// A human-readable explanation.
    pub message: String,
}

impl ValidationError {
    /// A field-level error.
    pub fn new(
        field: impl Into<String>,
        code: impl Into<Cow<'static, str>>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            field: Some(field.into()),
            code: code.into(),
            message: message.into(),
        }
    }

    /// An object-level error not tied to a specific field.
    pub fn global(code: impl Into<Cow<'static, str>>, message: impl Into<String>) -> Self {
        Self {
            field: None,
            code: code.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.field {
            Some(field) => write!(f, "{field}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

/// A collection of [`ValidationError`]s gathered from one validation pass.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ValidationErrors(Vec<ValidationError>);

impl ValidationErrors {
    /// An empty error set.
    pub fn new() -> Self {
        Self(Vec::new())
    }

    /// Whether no errors were recorded.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The number of recorded errors.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// The recorded errors.
    pub fn errors(&self) -> &[ValidationError] {
        &self.0
    }

    /// Record a pre-built error.
    pub fn push(&mut self, error: ValidationError) {
        self.0.push(error);
    }

    /// Record a field-level error.
    pub fn add(
        &mut self,
        field: impl Into<String>,
        code: impl Into<Cow<'static, str>>,
        message: impl Into<String>,
    ) {
        self.0.push(ValidationError::new(field, code, message));
    }

    /// Record an object-level error.
    pub fn add_global(
        &mut self,
        code: impl Into<Cow<'static, str>>,
        message: impl Into<String>,
    ) {
        self.0.push(ValidationError::global(code, message));
    }

    /// Absorb another set's errors.
    pub fn merge(&mut self, other: ValidationErrors) {
        self.0.extend(other.0);
    }

    /// Collapse into a `Result`: `Ok(())` when empty, otherwise `Err(self)`.
    pub fn into_result(self) -> Result<(), ValidationErrors> {
        if self.is_empty() {
            Ok(())
        } else {
            Err(self)
        }
    }
}

impl fmt::Display for ValidationErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "validation failed ({} error(s))", self.0.len())?;
        for (i, error) in self.0.iter().enumerate() {
            let sep = if i == 0 { ": " } else { "; " };
            write!(f, "{sep}{error}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ValidationErrors {}

impl DomainError for ValidationErrors {
    fn category(&self) -> ErrorCategory {
        ErrorCategory::BadRequest
    }

    fn code(&self) -> ErrorCode {
        ErrorCode::new("validation.failed")
    }
}

impl FromIterator<ValidationError> for ValidationErrors {
    fn from_iter<I: IntoIterator<Item = ValidationError>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

/// Types that can validate their own invariants.
pub trait Validate {
    /// Check invariants, returning every problem found (not just the first).
    fn validate(&self) -> Result<(), ValidationErrors>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct SignUp {
        email: String,
        age: u8,
    }

    impl Validate for SignUp {
        fn validate(&self) -> Result<(), ValidationErrors> {
            let mut errors = ValidationErrors::new();
            if !self.email.contains('@') {
                errors.add("email", "invalid_email", "must be a valid email address");
            }
            if self.age < 18 {
                errors.add("age", "too_small", "must be at least 18");
            }
            errors.into_result()
        }
    }

    #[test]
    fn accumulates_all_errors() {
        let err = SignUp {
            email: "nope".into(),
            age: 10,
        }
        .validate()
        .unwrap_err();

        assert_eq!(err.len(), 2);
        assert_eq!(err.errors()[0].field.as_deref(), Some("email"));
        assert_eq!(err.errors()[1].code, "too_small");
    }

    #[test]
    fn valid_input_passes() {
        assert!(
            SignUp {
                email: "a@b.co".into(),
                age: 21
            }
            .validate()
            .is_ok()
        );
    }

    #[test]
    fn is_a_bad_request_domain_error() {
        let err = SignUp {
            email: "x".into(),
            age: 5,
        }
        .validate()
        .unwrap_err();
        assert_eq!(err.category(), ErrorCategory::BadRequest);
        assert_eq!(err.code().as_str(), "validation.failed");
        assert_eq!(err.http_status(), 400);
    }

    #[test]
    fn merge_and_global_errors() {
        let mut a = ValidationErrors::new();
        a.add_global("conflict", "fields are mutually exclusive");
        let mut b = ValidationErrors::new();
        b.add("name", "required", "is required");
        a.merge(b);
        assert_eq!(a.len(), 2);
        assert!(a.errors()[0].field.is_none());
    }

    #[test]
    fn serializes_to_array() {
        let mut errors = ValidationErrors::new();
        errors.add("email", "required", "is required");
        let json = serde_json::to_string(&errors).unwrap();
        assert!(json.starts_with('['));
        assert!(json.contains("\"code\":\"required\""));
    }
}
