//! HTTP rendering for [`AppError`]: its actix [`ResponseError`] impl and the
//! uniform JSON body shape returned to clients.

use actix_web::http::StatusCode;
use actix_web::{HttpResponse, ResponseError};
use serde::Serialize;

use super::AppError;

#[derive(Serialize)]
struct ErrorBody<'a> {
    error: ErrorDetail<'a>,
}

#[derive(Serialize)]
struct ErrorDetail<'a> {
    code: &'a str,
    category: &'a str,
    message: String,
}

impl ResponseError for AppError {
    fn status_code(&self) -> StatusCode {
        StatusCode::from_u16(self.category().http_status())
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
    }

    fn error_response(&self) -> HttpResponse {
        let category = self.category();
        let code = self.code();
        let client_facing = category.is_client_error();

        if client_facing {
            tracing::debug!(code = %code, category = %category, "request rejected: {}", self.message());
        } else {
            tracing::error!(code = %code, category = %category, "request failed: {}", self.message());
        }

        // Never leak server-side detail to the client; the real message is logged above.
        let message = if client_facing {
            self.message().to_owned()
        } else {
            "internal server error".to_owned()
        };

        HttpResponse::build(self.status_code()).json(ErrorBody {
            error: ErrorDetail { code: code.as_str(), category: category.as_str(), message },
        })
    }
}
