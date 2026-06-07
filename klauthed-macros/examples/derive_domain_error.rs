//! Derive `DomainError` for a custom error enum and inspect the generated
//! classification.
//!
//! Run with: `cargo run -p klauthed-macros --example derive_domain_error`

use klauthed_error::DomainError;
use klauthed_macros::DomainError;

#[derive(Debug, DomainError)]
#[domain(prefix = "billing", category = "internal")]
enum BillingError {
    // category from the attr; code defaults to snake_case(variant) →
    // "billing.invoice_not_found"
    #[domain(category = "not_found")]
    InvoiceNotFound,
    // explicit code → "billing.declined"
    #[domain(category = "unprocessable_entity", code = "declined")]
    PaymentDeclined,
    // no per-variant attr → inherits category "internal", code "billing.ledger_io"
    LedgerIo,
}

impl std::fmt::Display for BillingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BillingError::InvoiceNotFound => f.write_str("invoice not found"),
            BillingError::PaymentDeclined => f.write_str("payment was declined"),
            BillingError::LedgerIo => f.write_str("ledger I/O failed"),
        }
    }
}

impl std::error::Error for BillingError {}

fn main() {
    let errors =
        [BillingError::InvoiceNotFound, BillingError::PaymentDeclined, BillingError::LedgerIo];
    for err in &errors {
        let code = err.code();
        println!("{:<24} http={}", code.as_str(), err.http_status());
    }
}
