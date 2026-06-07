//! Webhook endpoints, events, and HMAC-SHA256 signing.
//!
//! A [`WebhookEndpoint`] is a customer-registered URL plus a shared `secret` and
//! a set of subscribed event types. A [`WebhookEvent`] is the payload delivered to
//! it. Each delivery is signed with HMAC-SHA256 (via [`ring`], no hand-rolled
//! crypto) over `"{timestamp}.{body}"`, and the signature is rendered in the
//! Stripe-style header form `t=<unix_secs>,v1=<hex>`. The receiver recomputes the
//! MAC and compares it in constant time with [`verify_signature`].
//!
//! The default path is **types + signing + a trait sender** — no network. The
//! provided [`RecordingWebhookSender`] computes and attaches the signature and
//! captures every delivery for assertions. A real, `reqwest`-backed HTTP sender is
//! intentionally out of scope here (future work, ideally behind an optional
//! feature) so this crate stays dependency-light and offline-testable.
//!
//! ```
//! use klauthed_platform::webhooks::{sign_payload, verify_signature};
//!
//! let secret = b"whsec_test";
//! let body = r#"{"hello":"world"}"#;
//! let header = sign_payload(secret, 1_700_000_000, body.as_bytes());
//! assert!(header.starts_with("t=1700000000,v1="));
//!
//! // The receiver verifies with the same secret...
//! assert!(verify_signature(secret, &header, body.as_bytes()).is_ok());
//! // ...and a tampered body is rejected.
//! assert!(verify_signature(secret, &header, b"{}").is_err());
//! ```
//!
//! Future work (out of scope here): a `reqwest`-based [`WebhookSender`] behind a
//! `http` feature, automatic retries with backoff (reuse [`crate::jobs`]),
//! per-endpoint delivery metering, and notifications on repeated failure.

pub mod model;
pub mod sender;
pub mod signing;

pub use model::{
    WebhookEndpoint, WebhookEndpointId, WebhookEndpointMarker, WebhookEvent, WebhookEventId,
    WebhookEventMarker,
};
#[cfg(feature = "webhook-http")]
pub use sender::HttpWebhookSender;
pub use sender::{RecordingWebhookSender, WebhookDelivery, WebhookSender};
pub use signing::{SIGNATURE_VERSION, sign_payload, verify_signature};
