//! Request body extractors that speak [`AppError`].
//!
//! actix's built-in `web::Json<T>` rejects malformed bodies with its own error
//! type and JSON shape. These extractors instead funnel every failure through
//! [`AppError`] so the wire format stays uniform across the service:
//!
//! * [`Json<T>`] — deserialize a JSON body, mapping any parse/content-type
//!   problem to [`AppError::bad_request`].
//! * [`Validated<T>`] — like [`Json`], then runs [`Validate`] on the value,
//!   surfacing [`ValidationErrors`](klauthed_core::validation::ValidationErrors)
//!   (a `BadRequest` [`DomainError`](klauthed_error::DomainError)) as an
//!   [`AppError`] when invalid.
//!
//! ```no_run
//! use klauthed_web::extract::{Json, Validated};
//! use klauthed_core::validation::{Validate, ValidationErrors};
//! use serde::Deserialize;
//!
//! #[derive(Deserialize)]
//! struct CreateUser { email: String }
//!
//! impl Validate for CreateUser {
//!     fn validate(&self) -> Result<(), ValidationErrors> {
//!         let mut errs = ValidationErrors::new();
//!         if !self.email.contains('@') {
//!             errs.add("email", "invalid_email", "must contain '@'");
//!         }
//!         errs.into_result()
//!     }
//! }
//!
//! // `async fn handler(body: Json<CreateUser>) -> ...`
//! // `async fn handler(body: Validated<CreateUser>) -> ...`
//! ```

use std::ops::{Deref, DerefMut};

use actix_web::dev::Payload;
use actix_web::http::header;
use actix_web::{FromRequest, HttpRequest};
use futures_util::future::LocalBoxFuture;
use klauthed_core::validation::Validate;
use serde::de::DeserializeOwned;

use crate::error::AppError;

/// JSON body extractor that maps deserialization failures to
/// [`AppError::bad_request`].
///
/// Deref to `T`; [`Json::into_inner`] takes ownership of the parsed value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Json<T>(pub T);

impl<T> Json<T> {
    /// Consume the wrapper and return the parsed value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> Deref for Json<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for Json<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> FromRequest for Json<T>
where
    T: DeserializeOwned + 'static,
{
    type Error = AppError;
    type Future = LocalBoxFuture<'static, Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
        let fut = parse_json::<T>(req, payload);
        Box::pin(async move { fut.await.map(Json) })
    }
}

/// JSON body extractor that deserializes then [`Validate`]s the value.
///
/// A malformed body yields the same `BadRequest` as [`Json`]; a well-formed but
/// invalid body yields the type's [`ValidationErrors`](klauthed_core::validation::ValidationErrors)
/// as a `BadRequest` [`AppError`].
///
/// Deref to `T`; [`Validated::into_inner`] takes ownership of the value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Validated<T>(pub T);

impl<T> Validated<T> {
    /// Consume the wrapper and return the validated value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> Deref for Validated<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for Validated<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> FromRequest for Validated<T>
where
    T: DeserializeOwned + Validate + 'static,
{
    type Error = AppError;
    type Future = LocalBoxFuture<'static, Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
        let fut = parse_json::<T>(req, payload);
        Box::pin(async move {
            let value = fut.await?;
            value.validate().map_err(AppError::from_domain)?;
            Ok(Validated(value))
        })
    }
}

/// Read the full body and deserialize it as JSON, mapping every failure to a
/// `BadRequest` [`AppError`]. Shared by both extractors.
fn parse_json<T>(
    req: &HttpRequest,
    payload: &mut Payload,
) -> LocalBoxFuture<'static, Result<T, AppError>>
where
    T: DeserializeOwned + 'static,
{
    // Reject an explicit non-JSON content type early with a clear message.
    if let Some(mime) = req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        && !is_json_content_type(mime)
    {
        let mime = mime.to_owned();
        return Box::pin(async move {
            Err(AppError::bad_request(format!(
                "expected `application/json` content type, got `{mime}`"
            )))
        });
    }

    let bytes_fut = actix_web::web::Bytes::from_request(req, payload);
    Box::pin(async move {
        let bytes = bytes_fut
            .await
            .map_err(|e| AppError::bad_request(format!("could not read request body: {e}")))?;
        serde_json::from_slice::<T>(&bytes)
            .map_err(|e| AppError::bad_request(format!("invalid JSON body: {e}")))
    })
}

/// Whether a `Content-Type` header value denotes JSON (`application/json` or any
/// `+json` structured suffix), ignoring parameters like `; charset=utf-8`.
fn is_json_content_type(value: &str) -> bool {
    let essence = value.split(';').next().unwrap_or(value).trim();
    essence.eq_ignore_ascii_case("application/json") || essence.to_ascii_lowercase().ends_with("+json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::http::StatusCode;
    use actix_web::{test, web, App, HttpResponse, ResponseError};
    use klauthed_core::validation::ValidationErrors;
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct CreateUser {
        email: String,
        age: u8,
    }

    impl Validate for CreateUser {
        fn validate(&self) -> Result<(), ValidationErrors> {
            let mut errs = ValidationErrors::new();
            if !self.email.contains('@') {
                errs.add("email", "invalid_email", "must contain '@'");
            }
            if self.age < 18 {
                errs.add("age", "too_small", "must be at least 18");
            }
            errs.into_result()
        }
    }

    #[std::prelude::v1::test]
    fn json_content_type_detection() {
        assert!(is_json_content_type("application/json"));
        assert!(is_json_content_type("application/json; charset=utf-8"));
        assert!(is_json_content_type("application/merge-patch+json"));
        assert!(!is_json_content_type("text/plain"));
        assert!(!is_json_content_type("application/xml"));
    }

    async fn json_handler(body: Json<CreateUser>) -> HttpResponse {
        HttpResponse::Ok().body(body.into_inner().email)
    }

    async fn validated_handler(body: Validated<CreateUser>) -> HttpResponse {
        HttpResponse::Ok().body(body.into_inner().email)
    }

    #[actix_web::test]
    async fn json_accepts_valid_body() {
        let app =
            test::init_service(App::new().route("/", web::post().to(json_handler))).await;
        let req = test::TestRequest::post()
            .uri("/")
            .set_json(serde_json::json!({ "email": "a@b.co", "age": 30 }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[actix_web::test]
    async fn json_rejects_malformed_body_with_400() {
        let app =
            test::init_service(App::new().route("/", web::post().to(json_handler))).await;
        let req = test::TestRequest::post()
            .uri("/")
            .insert_header((header::CONTENT_TYPE, "application/json"))
            .set_payload("{ not json ")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let body = test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["category"], "bad_request");
    }

    #[actix_web::test]
    async fn json_rejects_wrong_content_type() {
        let err = {
            // Drive the extractor directly to assert the produced AppError.
            let (req, mut payload) = test::TestRequest::post()
                .insert_header((header::CONTENT_TYPE, "text/plain"))
                .set_payload("hello")
                .to_http_parts();
            Json::<CreateUser>::from_request(&req, &mut payload)
                .await
                .unwrap_err()
        };
        assert_eq!(err.error_response().status(), StatusCode::BAD_REQUEST);
    }

    #[actix_web::test]
    async fn validated_accepts_valid_input() {
        let app =
            test::init_service(App::new().route("/", web::post().to(validated_handler))).await;
        let req = test::TestRequest::post()
            .uri("/")
            .set_json(serde_json::json!({ "email": "a@b.co", "age": 30 }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[actix_web::test]
    async fn validated_rejects_invalid_input_with_400() {
        let app =
            test::init_service(App::new().route("/", web::post().to(validated_handler))).await;
        let req = test::TestRequest::post()
            .uri("/")
            .set_json(serde_json::json!({ "email": "nope", "age": 10 }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let body = test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["category"], "bad_request");
        assert_eq!(json["error"]["code"], "validation.failed");
    }
}
