//! Integration tests for `#[derive(DomainError)]`.
//!
//! These run as a separate crate that depends on both `klauthed-macros` and
//! `klauthed-error`, exercising the generated code end to end.
#![allow(dead_code)] // some variant fields exist only to model realistic errors

use klauthed_error::{DomainError, ErrorCategory};
use klauthed_macros::DomainError;

// A wrapped error to exercise `transparent` delegation.
#[derive(Debug, DomainError)]
#[domain(prefix = "inner")]
enum InnerError {
    #[domain(category = "not_found")]
    Gone,
}
impl std::fmt::Display for InnerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("inner error")
    }
}
impl std::error::Error for InnerError {}

#[derive(Debug, DomainError)]
#[domain(prefix = "demo")]
enum DemoError {
    // category from attr; code defaults to snake_case(variant) → "demo.missing"
    #[domain(category = "not_found")]
    Missing,
    // explicit code → "demo.already_there"
    #[domain(category = "conflict", code = "already_there")]
    Duplicate(String),
    // struct variant; code defaults → "demo.invalid"
    #[domain(category = "bad_request")]
    Invalid { reason: String },
    // no #[domain] attr at all → category internal, code "demo.defaulted"
    Defaulted,
    // delegates category()/code() to the wrapped DomainError
    #[domain(transparent)]
    Inner(InnerError),
}
impl std::fmt::Display for DemoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for DemoError {}

#[test]
fn category_and_code_from_attributes() {
    let missing = DemoError::Missing;
    assert_eq!(missing.category(), ErrorCategory::NotFound);
    assert_eq!(missing.code().as_str(), "demo.missing");
    assert_eq!(missing.http_status(), 404);

    let dup = DemoError::Duplicate("x".into());
    assert_eq!(dup.category(), ErrorCategory::Conflict);
    assert_eq!(dup.code().as_str(), "demo.already_there");

    let invalid = DemoError::Invalid {
        reason: "bad".into(),
    };
    assert_eq!(invalid.category(), ErrorCategory::BadRequest);
    assert_eq!(invalid.code().as_str(), "demo.invalid");
}

#[test]
fn defaults_to_internal_and_snake_cased_code() {
    let err = DemoError::Defaulted;
    assert_eq!(err.category(), ErrorCategory::Internal);
    assert_eq!(err.code().as_str(), "demo.defaulted");
}

#[test]
fn transparent_delegates_to_wrapped_error() {
    let err = DemoError::Inner(InnerError::Gone);
    // Category and code come from InnerError, not DemoError.
    assert_eq!(err.category(), ErrorCategory::NotFound);
    assert_eq!(err.code().as_str(), "inner.gone");
}

// A struct error: one category/code for the whole type.
#[derive(Debug, DomainError)]
#[domain(category = "unavailable", code = "upstream.down")]
struct UpstreamDown;
impl std::fmt::Display for UpstreamDown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("upstream down")
    }
}
impl std::error::Error for UpstreamDown {}

#[test]
fn struct_error_uses_container_attrs() {
    let err = UpstreamDown;
    assert_eq!(err.category(), ErrorCategory::Unavailable);
    assert_eq!(err.code().as_str(), "upstream.down");
    assert!(err.is_retryable());
}

// Acronym variants: the old hand-rolled snake_case split every capital letter
// individually (HTTPError → "h_t_t_p_error"). heck handles runs of uppercase
// correctly: "HTTPError" → "http_error", "APIKey" → "api_key".
#[derive(Debug, DomainError)]
#[domain(prefix = "api")]
enum AcronymError {
    #[domain(category = "bad_request")]
    HTTPNotFound,
    #[domain(category = "unauthorized")]
    APIKeyMissing,
    #[domain(category = "internal")]
    ParseURLFailed,
}
impl std::fmt::Display for AcronymError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for AcronymError {}

/// Valid codes: bare suffix, fully-qualified with dot, digits, underscores.
#[derive(Debug, DomainError)]
#[domain(prefix = "svc")]
enum ValidCodes {
    #[domain(category = "not_found", code = "resource_not_found")]
    WithUnderscore,
    #[domain(category = "internal", code = "step2_failed")]
    WithDigit,
    // Bare variant — auto-converted to snake_case.
    ConnectionTimeout,
}
impl std::fmt::Display for ValidCodes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ValidCodes {}

/// Fully-qualified code with no prefix — dot is valid here.
#[derive(Debug, DomainError)]
#[domain(category = "unavailable", code = "infra.circuit_open")]
struct CircuitOpen;
impl std::fmt::Display for CircuitOpen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("circuit open")
    }
}
impl std::error::Error for CircuitOpen {}

#[test]
fn valid_codes_compile_and_produce_correct_strings() {
    assert_eq!(
        ValidCodes::WithUnderscore.code().as_str(),
        "svc.resource_not_found"
    );
    assert_eq!(ValidCodes::WithDigit.code().as_str(), "svc.step2_failed");
    assert_eq!(
        ValidCodes::ConnectionTimeout.code().as_str(),
        "svc.connection_timeout"
    );
    // Fully-qualified code preserves the dot.
    assert_eq!(CircuitOpen.code().as_str(), "infra.circuit_open");
}

#[test]
fn acronym_variants_snake_case_correctly() {
    // These would have been "h_t_t_p_not_found", "a_p_i_key_missing",
    // "parse_u_r_l_failed" with the old hand-rolled implementation.
    assert_eq!(AcronymError::HTTPNotFound.code().as_str(), "api.http_not_found");
    assert_eq!(AcronymError::APIKeyMissing.code().as_str(), "api.api_key_missing");
    assert_eq!(AcronymError::ParseURLFailed.code().as_str(), "api.parse_url_failed");
}
