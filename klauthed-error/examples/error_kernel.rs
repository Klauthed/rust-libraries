//! Implement the `DomainError` contract for a custom error type and inspect the
//! shared classification (HTTP status, retryability, stable code).
//!
//! Run with: `cargo run -p klauthed-error --example error_kernel`

use klauthed_error::{DomainError, ErrorCategory, ErrorCode};

#[derive(Debug)]
enum OrderError {
    NotFound,
    PaymentDeclined,
    LedgerUnavailable,
}

impl std::fmt::Display for OrderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrderError::NotFound => f.write_str("order not found"),
            OrderError::PaymentDeclined => f.write_str("payment was declined"),
            OrderError::LedgerUnavailable => f.write_str("ledger service is unavailable"),
        }
    }
}

impl std::error::Error for OrderError {}

impl DomainError for OrderError {
    fn category(&self) -> ErrorCategory {
        match self {
            OrderError::NotFound => ErrorCategory::NotFound,
            OrderError::PaymentDeclined => ErrorCategory::UnprocessableEntity,
            OrderError::LedgerUnavailable => ErrorCategory::Unavailable,
        }
    }

    fn code(&self) -> ErrorCode {
        match self {
            OrderError::NotFound => ErrorCode::new("order.not_found"),
            OrderError::PaymentDeclined => ErrorCode::new("order.payment_declined"),
            OrderError::LedgerUnavailable => ErrorCode::new("order.ledger_unavailable"),
        }
    }
}

fn main() {
    let errors = [OrderError::NotFound, OrderError::PaymentDeclined, OrderError::LedgerUnavailable];

    println!("{:<32} {:<28} {:>4}  retryable", "error", "code", "http");
    for err in &errors {
        let code = err.code();
        println!(
            "{err:<32} {:<28} {:>4}  {}",
            code.as_str(),
            err.http_status(),
            err.is_retryable(),
        );
    }
}
