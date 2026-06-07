//! Public-API integration tests for [`AppError`]: status mapping, `From`
//! conversions, and the JSON error body (including 5xx message hiding).

use std::error::Error;

use actix_web::ResponseError;
use actix_web::body::to_bytes;
use actix_web::http::StatusCode;
use klauthed_error::ErrorCategory;
use klauthed_web::AppError;

#[test]
fn status_maps_from_category() {
    assert_eq!(AppError::not_found("x").status_code(), StatusCode::NOT_FOUND);
    assert_eq!(AppError::bad_request("x").status_code().as_u16(), 400);
    assert_eq!(AppError::internal("x").status_code(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn from_config_error_preserves_code_and_category() {
    let err: AppError =
        klauthed_core::error::ConfigError::MissingRequired("database".into()).into();
    assert_eq!(err.code().as_str(), "config.missing_required");
    assert_eq!(err.category(), ErrorCategory::Internal);
    assert!(err.source().is_some());
}

#[test]
fn from_data_error_is_unavailable_for_unsupported() {
    let err: AppError =
        klauthed_data::DataError::UnsupportedSystem(klauthed_core::config::DbSystem::MongoDb)
            .into();
    assert_eq!(err.code().as_str(), "data.unsupported_system");
}

#[actix_web::test]
async fn client_error_body_includes_message() {
    let resp = AppError::bad_request("field 'name' is required").error_response();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let bytes = to_bytes(resp.into_body()).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["error"]["code"], "request.bad_request");
    assert_eq!(body["error"]["category"], "bad_request");
    assert_eq!(body["error"]["message"], "field 'name' is required");
}

#[test]
fn from_security_error_preserves_code_and_category() {
    let err: AppError = klauthed_security::SecurityError::ExpiredToken.into();
    assert_eq!(err.code().as_str(), "security.expired_token");
    assert_eq!(err.category(), ErrorCategory::Unauthorized);
    assert_eq!(err.status_code().as_u16(), 401);
    assert!(err.source().is_some());
}

#[test]
fn from_security_invalid_token_is_bad_request() {
    let err: AppError = klauthed_security::SecurityError::MalformedToken("bad jwt".into()).into();
    assert_eq!(err.code().as_str(), "security.malformed_token");
    assert_eq!(err.category(), ErrorCategory::BadRequest);
}

#[test]
fn from_platform_error_preserves_code_and_category() {
    let err: AppError =
        klauthed_platform::PlatformError::TenantNotFound { id_or_slug: "acme".into() }.into();
    assert_eq!(err.code().as_str(), "platform.tenant_not_found");
    assert_eq!(err.category(), ErrorCategory::NotFound);
    assert_eq!(err.status_code().as_u16(), 404);
    assert!(err.source().is_some());
}

#[test]
fn from_platform_backend_is_internal() {
    let err: AppError =
        klauthed_platform::PlatformError::Backend { message: "db conn failed".into() }.into();
    assert_eq!(err.category(), ErrorCategory::Internal);
}

#[test]
fn unprocessable_entity_constructor_gives_422() {
    let err = AppError::unprocessable_entity("email must be a valid address");
    assert_eq!(err.status_code().as_u16(), 422);
    assert_eq!(err.category(), ErrorCategory::UnprocessableEntity);
    assert_eq!(err.code().as_str(), "request.unprocessable_entity");
}

#[test]
fn validation_errors_surface_as_422() {
    use klauthed_core::validation::{Validate, ValidationErrors};
    struct Bad;
    impl Validate for Bad {
        fn validate(&self) -> Result<(), ValidationErrors> {
            let mut e = ValidationErrors::new();
            e.add("name", "required", "name is required");
            e.into_result()
        }
    }
    let app_err: AppError = Bad.validate().unwrap_err().into();
    assert_eq!(app_err.status_code().as_u16(), 422);
    assert_eq!(app_err.code().as_str(), "validation.failed");
}

#[actix_web::test]
async fn server_error_body_hides_message_but_keeps_code() {
    // A sensitive internal message must not reach the client.
    let resp = AppError::internal("db password = hunter2").error_response();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let bytes = to_bytes(resp.into_body()).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["error"]["message"], "internal server error");
    assert_eq!(body["error"]["code"], "request.internal");
    assert_eq!(body["error"]["category"], "internal");
}
