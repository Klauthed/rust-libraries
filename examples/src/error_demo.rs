//! `klauthed-error` + `klauthed-macros`: the `DomainError` derive generates the
//! category + stable `prefix.code` from `#[domain(...)]` attributes.

use klauthed_error::{DomainError, ErrorCategory};
use klauthed_macros::DomainError;

#[derive(Debug, DomainError)]
#[domain(prefix = "billing")]
enum BillingError {
    /// Defaults the code to the snake-cased variant name -> `billing.card_declined`.
    #[domain(category = "bad_request")]
    CardDeclined,
    #[domain(category = "not_found", code = "no_such_invoice")]
    InvoiceMissing(String),
    #[domain(category = "unavailable")]
    Gateway,
}

impl std::fmt::Display for BillingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BillingError::CardDeclined => f.write_str("card declined"),
            BillingError::InvoiceMissing(id) => write!(f, "no invoice {id}"),
            BillingError::Gateway => f.write_str("payment gateway unavailable"),
        }
    }
}
impl std::error::Error for BillingError {}

pub fn run() {
    for err in [
        BillingError::CardDeclined,
        BillingError::InvoiceMissing("inv_42".into()),
        BillingError::Gateway,
    ] {
        println!(
            "  {:<28} -> code={:<24} category={:?} http={}",
            err.to_string(),
            err.code().as_str(),
            err.category(),
            err.category().http_status(),
        );
    }

    assert_eq!(BillingError::CardDeclined.code().as_str(), "billing.card_declined");
    assert_eq!(BillingError::CardDeclined.category(), ErrorCategory::BadRequest);
    assert_eq!(BillingError::InvoiceMissing("x".into()).code().as_str(), "billing.no_such_invoice");
    assert_eq!(BillingError::Gateway.category().http_status(), 503);
}
