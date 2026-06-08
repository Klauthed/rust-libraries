//! The [`ValidationError`] item and the [`ValidationErrors`] collection.

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
        Self { field: Some(field.into()), code: code.into(), message: message.into() }
    }

    /// An object-level error not tied to a specific field.
    pub fn global(code: impl Into<Cow<'static, str>>, message: impl Into<String>) -> Self {
        Self { field: None, code: code.into(), message: message.into() }
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
    pub fn add_global(&mut self, code: impl Into<Cow<'static, str>>, message: impl Into<String>) {
        self.0.push(ValidationError::global(code, message));
    }

    /// Absorb another set's errors.
    pub fn merge(&mut self, other: ValidationErrors) {
        self.0.extend(other.0);
    }

    /// Collapse into a `Result`: `Ok(())` when empty, otherwise `Err(self)`.
    pub fn into_result(self) -> Result<(), ValidationErrors> {
        if self.is_empty() { Ok(()) } else { Err(self) }
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
        // 422, not 400: the request was well-formed but semantically invalid.
        // Use UnprocessableEntity so clients (and HTTP tooling) can distinguish
        // "your JSON was garbled" from "your value failed a business rule".
        ErrorCategory::UnprocessableEntity
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
