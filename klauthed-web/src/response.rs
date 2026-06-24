//! A uniform success envelope, [`ApiResponse`], pairing with [`AppError`].
//!
//! Handlers return [`ApiResult<T>`]: the `Ok` arm renders as `{ "data": <T> }`
//! and the `Err` arm renders as [`AppError`]'s `{ "error": { … } }` body — a
//! symmetric success/failure contract across every service, so a client always
//! branches on `data` vs `error`.
//!
//! ```
//! use klauthed_web::{ApiResponse, ApiResult};
//! use serde::Serialize;
//!
//! #[derive(Serialize)]
//! struct User {
//!     id: u64,
//!     name: String,
//! }
//!
//! // 200 OK, body: {"data":{"id":1,"name":"Ada"}}
//! async fn get_user() -> ApiResult<User> {
//!     Ok(ApiResponse::ok(User { id: 1, name: "Ada".into() }))
//! }
//!
//! // 201 Created
//! async fn create_user() -> ApiResult<User> {
//!     Ok(ApiResponse::created(User { id: 2, name: "Grace".into() }))
//! }
//! ```

use actix_web::body::BoxBody;
use actix_web::http::StatusCode;
use actix_web::{HttpRequest, HttpResponse, Responder};
use serde::Serialize;

use crate::error::AppError;

/// A uniform success envelope rendered as `{ "data": <T> }` — the success
/// counterpart to [`AppError`]'s `{ "error": { … } }` body.
///
/// Implements actix [`Responder`], so a handler can return it directly (or, more
/// commonly, return [`ApiResult<T>`] so the failure path renders an [`AppError`]).
#[derive(Debug, Clone)]
pub struct ApiResponse<T> {
    data: T,
    status: StatusCode,
}

impl<T> ApiResponse<T> {
    /// A `200 OK` response wrapping `data`.
    #[must_use]
    pub fn ok(data: T) -> Self {
        Self { data, status: StatusCode::OK }
    }

    /// A `201 Created` response wrapping `data`.
    #[must_use]
    pub fn created(data: T) -> Self {
        Self { data, status: StatusCode::CREATED }
    }

    /// Override the HTTP status (default `200 OK`).
    #[must_use]
    pub fn with_status(mut self, status: StatusCode) -> Self {
        self.status = status;
        self
    }

    /// Borrow the wrapped value.
    pub fn data(&self) -> &T {
        &self.data
    }

    /// The HTTP status this will render with.
    pub fn status(&self) -> StatusCode {
        self.status
    }
}

impl<T: Serialize> Responder for ApiResponse<T> {
    type Body = BoxBody;

    fn respond_to(self, _req: &HttpRequest) -> HttpResponse {
        #[derive(Serialize)]
        struct Envelope<T> {
            data: T,
        }
        HttpResponse::build(self.status).json(Envelope { data: self.data })
    }
}

/// A handler result with symmetric envelopes: `Ok(ApiResponse)` renders as
/// `{ "data": … }`, `Err(AppError)` as `{ "error": { … } }`.
pub type ApiResult<T> = Result<ApiResponse<T>, AppError>;

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::body::to_bytes;
    use actix_web::test::TestRequest;

    #[derive(Serialize)]
    struct Dto {
        id: u64,
        name: &'static str,
    }

    #[actix_web::test]
    async fn ok_renders_data_envelope_with_200() {
        let resp = ApiResponse::ok(Dto { id: 1, name: "Ada" })
            .respond_to(&TestRequest::default().to_http_request());
        assert_eq!(resp.status(), StatusCode::OK);
        let body = to_bytes(resp.into_body()).await.unwrap();
        assert_eq!(body, r#"{"data":{"id":1,"name":"Ada"}}"#);
    }

    #[actix_web::test]
    async fn created_uses_201() {
        let resp = ApiResponse::created(Dto { id: 2, name: "Grace" })
            .respond_to(&TestRequest::default().to_http_request());
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    #[actix_web::test]
    async fn with_status_overrides() {
        let resp = ApiResponse::ok(Dto { id: 3, name: "x" })
            .with_status(StatusCode::ACCEPTED)
            .respond_to(&TestRequest::default().to_http_request());
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
    }
}
