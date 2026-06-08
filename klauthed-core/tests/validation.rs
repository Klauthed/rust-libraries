//! Public-API integration tests for validation: error accumulation, the
//! `Validate` trait, serde shape, and the DomainError category/code.

use klauthed_core::validation::{Validate, ValidationErrors};
use klauthed_error::{DomainError, ErrorCategory};

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
    let err = SignUp { email: "nope".into(), age: 10 }.validate().unwrap_err();

    assert_eq!(err.len(), 2);
    assert_eq!(err.errors()[0].field.as_deref(), Some("email"));
    assert_eq!(err.errors()[1].code, "too_small");
}

#[test]
fn valid_input_passes() {
    assert!(SignUp { email: "a@b.co".into(), age: 21 }.validate().is_ok());
}

#[test]
fn validation_errors_are_unprocessable_entity() {
    let err = SignUp { email: "x".into(), age: 5 }.validate().unwrap_err();
    assert_eq!(err.category(), ErrorCategory::UnprocessableEntity);
    assert_eq!(err.code().as_str(), "validation.failed");
    // 422 Unprocessable Entity, NOT 400 Bad Request.
    assert_eq!(err.http_status(), 422);
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
