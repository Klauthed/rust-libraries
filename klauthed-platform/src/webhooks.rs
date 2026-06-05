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

use std::sync::Mutex;

use async_trait::async_trait;
use klauthed_core::id::Id;
use klauthed_core::time::Timestamp;
use ring::hmac;
use serde::{Deserialize, Serialize};

use crate::error::PlatformError;

/// Zero-sized marker tagging a [`WebhookEndpointId`].
pub struct WebhookEndpointMarker;

/// A typed identifier for a [`WebhookEndpoint`].
pub type WebhookEndpointId = Id<WebhookEndpointMarker>;

/// Zero-sized marker tagging a [`WebhookEventId`].
pub struct WebhookEventMarker;

/// A typed identifier for a [`WebhookEvent`].
pub type WebhookEventId = Id<WebhookEventMarker>;

/// The version tag emitted/required in the signature header (`v1=...`).
pub const SIGNATURE_VERSION: &str = "v1";

/// A registered webhook destination.
///
/// The `secret` is the shared key used to sign deliveries; treat it as sensitive
/// and never log it (its [`Debug`] is redacted).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhookEndpoint {
    id: WebhookEndpointId,
    url: String,
    secret: String,
    event_types: Vec<String>,
    active: bool,
}

impl WebhookEndpoint {
    /// A new, **active** endpoint with no event subscriptions and a fresh id.
    pub fn new(url: impl Into<String>, secret: impl Into<String>) -> Self {
        Self {
            id: WebhookEndpointId::new(),
            url: url.into(),
            secret: secret.into(),
            event_types: Vec::new(),
            active: true,
        }
    }

    /// Override the endpoint id (builder style).
    pub fn with_id(mut self, id: WebhookEndpointId) -> Self {
        self.id = id;
        self
    }

    /// Subscribe to an event type (builder style; duplicates are ignored).
    pub fn subscribe(mut self, event_type: impl Into<String>) -> Self {
        let ty = event_type.into();
        if !self.event_types.contains(&ty) {
            self.event_types.push(ty);
        }
        self
    }

    /// Set the active flag (builder style).
    pub fn with_active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    /// The endpoint id.
    pub fn id(&self) -> WebhookEndpointId {
        self.id
    }

    /// The destination URL.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// The shared signing secret.
    pub fn secret(&self) -> &str {
        &self.secret
    }

    /// The subscribed event types.
    pub fn event_types(&self) -> &[String] {
        &self.event_types
    }

    /// Whether the endpoint is active (eligible for delivery).
    pub fn active(&self) -> bool {
        self.active
    }

    /// Whether this endpoint should receive `event_type`: it must be active and
    /// either have no explicit subscriptions (receive-all) or list `event_type`.
    pub fn accepts(&self, event_type: &str) -> bool {
        self.active
            && (self.event_types.is_empty()
                || self.event_types.iter().any(|t| t == event_type))
    }
}

impl std::fmt::Debug for WebhookEndpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebhookEndpoint")
            .field("id", &self.id)
            .field("url", &self.url)
            .field("secret", &"<redacted>")
            .field("event_types", &self.event_types)
            .field("active", &self.active)
            .finish()
    }
}

/// An event to deliver to subscribed [`WebhookEndpoint`]s.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhookEvent {
    id: WebhookEventId,
    event_type: String,
    occurred_at: Timestamp,
    data: serde_json::Value,
}

impl WebhookEvent {
    /// A new event of `event_type` carrying `data`, with a fresh id.
    pub fn new(
        event_type: impl Into<String>,
        occurred_at: Timestamp,
        data: serde_json::Value,
    ) -> Self {
        Self {
            id: WebhookEventId::new(),
            event_type: event_type.into(),
            occurred_at,
            data,
        }
    }

    /// Override the event id (builder style).
    pub fn with_id(mut self, id: WebhookEventId) -> Self {
        self.id = id;
        self
    }

    /// The event id.
    pub fn id(&self) -> WebhookEventId {
        self.id
    }

    /// The event type (matched against [`WebhookEndpoint::event_types`]).
    pub fn event_type(&self) -> &str {
        &self.event_type
    }

    /// When the event occurred.
    pub fn occurred_at(&self) -> Timestamp {
        self.occurred_at
    }

    /// The event payload.
    pub fn data(&self) -> &serde_json::Value {
        &self.data
    }

    /// The canonical JSON body that gets signed and delivered.
    pub fn to_body(&self) -> Result<String, PlatformError> {
        serde_json::to_string(self).map_err(|e| PlatformError::WebhookSigning {
            message: format!("serialize event: {e}"),
        })
    }
}

/// Compute the signed-payload string `"{timestamp}.{body}"`, the exact bytes the
/// HMAC is taken over.
fn signing_input(timestamp_secs: i64, body: &[u8]) -> Vec<u8> {
    let mut input = Vec::with_capacity(body.len() + 16);
    input.extend_from_slice(timestamp_secs.to_string().as_bytes());
    input.push(b'.');
    input.extend_from_slice(body);
    input
}

/// Sign `body` with `secret` at `timestamp_secs` (Unix seconds) and return the
/// Stripe-style header value `t=<timestamp_secs>,v1=<hex>`.
///
/// The MAC is HMAC-SHA256 over `"{timestamp_secs}.{body}"`.
pub fn sign_payload(secret: &[u8], timestamp_secs: i64, body: &[u8]) -> String {
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret);
    let tag = hmac::sign(&key, &signing_input(timestamp_secs, body));
    format!(
        "t={timestamp_secs},{SIGNATURE_VERSION}={}",
        hex::encode(tag.as_ref())
    )
}

/// Parse a `t=<secs>,v1=<hex>` header into its timestamp and the `v1` hex MAC.
fn parse_signature_header(header: &str) -> Option<(i64, String)> {
    let mut timestamp = None;
    let mut v1 = None;
    for part in header.split(',') {
        let (k, v) = part.split_once('=')?;
        match k.trim() {
            "t" => timestamp = v.trim().parse::<i64>().ok(),
            SIGNATURE_VERSION => v1 = Some(v.trim().to_owned()),
            _ => {}
        }
    }
    Some((timestamp?, v1?))
}

/// Verify a `t=<secs>,v1=<hex>` `header` against `body` using `secret`.
///
/// Recomputes the HMAC over `"{t}.{body}"` and compares it to the supplied `v1`
/// MAC in **constant time** (via [`ring::hmac::verify`]). Returns
/// [`PlatformError::WebhookSigning`] on a malformed header and
/// [`PlatformError::WebhookDelivery`] on a signature mismatch.
pub fn verify_signature(
    secret: &[u8],
    header: &str,
    body: &[u8],
) -> Result<(), PlatformError> {
    let (timestamp, v1_hex) =
        parse_signature_header(header).ok_or_else(|| PlatformError::WebhookSigning {
            message: "malformed signature header".to_owned(),
        })?;

    let provided = hex::decode(&v1_hex).map_err(|_| PlatformError::WebhookSigning {
        message: "signature is not valid hex".to_owned(),
    })?;

    let key = hmac::Key::new(hmac::HMAC_SHA256, secret);
    hmac::verify(&key, &signing_input(timestamp, body), &provided).map_err(|_| {
        PlatformError::WebhookDelivery {
            message: "webhook signature mismatch".to_owned(),
        }
    })
}

/// A delivered (or attempted) webhook, as captured by [`RecordingWebhookSender`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebhookDelivery {
    /// The endpoint the event was delivered to.
    pub endpoint_id: WebhookEndpointId,
    /// The destination URL at delivery time.
    pub url: String,
    /// The event that was delivered.
    pub event: WebhookEvent,
    /// The serialized JSON body that was signed and sent.
    pub body: String,
    /// The `t=<secs>,v1=<hex>` signature header value attached to the delivery.
    pub signature: String,
}

/// A transport that delivers a signed [`WebhookEvent`] to a [`WebhookEndpoint`].
///
/// Implementors are `Send + Sync` so a sender can be shared as
/// `Arc<dyn WebhookSender>`.
#[async_trait]
pub trait WebhookSender: Send + Sync {
    /// Deliver `event` to `endpoint`. Implementations are expected to sign the
    /// body and attach the signature header.
    async fn deliver(
        &self,
        endpoint: &WebhookEndpoint,
        event: &WebhookEvent,
    ) -> Result<(), PlatformError>;
}

/// An in-memory [`WebhookSender`] that signs and records deliveries instead of
/// performing real network I/O — the offline default for tests and dry-runs.
///
/// The signing timestamp is taken from the event's
/// [`occurred_at`](WebhookEvent::occurred_at), keeping deliveries deterministic
/// under a [`FixedClock`](klauthed_core::time::FixedClock).
#[derive(Default)]
pub struct RecordingWebhookSender {
    deliveries: Mutex<Vec<WebhookDelivery>>,
}

impl RecordingWebhookSender {
    /// A new, empty recording sender.
    pub fn new() -> Self {
        Self::default()
    }

    /// A snapshot of all recorded deliveries, in delivery order.
    pub fn deliveries(&self) -> Vec<WebhookDelivery> {
        self.deliveries.lock().expect("webhook lock poisoned").clone()
    }

    /// The number of recorded deliveries.
    pub fn len(&self) -> usize {
        self.deliveries.lock().expect("webhook lock poisoned").len()
    }

    /// Whether nothing has been delivered yet.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[async_trait]
impl WebhookSender for RecordingWebhookSender {
    async fn deliver(
        &self,
        endpoint: &WebhookEndpoint,
        event: &WebhookEvent,
    ) -> Result<(), PlatformError> {
        if !endpoint.active() {
            return Err(PlatformError::WebhookDelivery {
                message: format!("endpoint {} is inactive", endpoint.id()),
            });
        }

        let body = event.to_body()?;
        let timestamp_secs = event.occurred_at().unix_millis() / 1_000;
        let signature = sign_payload(endpoint.secret().as_bytes(), timestamp_secs, body.as_bytes());

        self.deliveries
            .lock()
            .expect("webhook lock poisoned")
            .push(WebhookDelivery {
                endpoint_id: endpoint.id(),
                url: endpoint.url().to_owned(),
                event: event.clone(),
                body,
                signature,
            });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event() -> WebhookEvent {
        WebhookEvent::new(
            "invoice.paid",
            Timestamp::from_unix_millis(1_700_000_000_000),
            serde_json::json!({ "amount": 4200 }),
        )
        .with_id(WebhookEventId::nil())
    }

    #[test]
    fn signature_is_deterministic_for_fixed_inputs() {
        let secret = b"whsec_abc";
        let body = br#"{"k":"v"}"#;
        let a = sign_payload(secret, 1_700_000_000, body);
        let b = sign_payload(secret, 1_700_000_000, body);
        assert_eq!(a, b);
        assert!(a.starts_with("t=1700000000,v1="));
    }

    #[test]
    fn verify_accepts_valid_and_rejects_tampered() {
        let secret = b"whsec_abc";
        let body = br#"{"k":"v"}"#;
        let header = sign_payload(secret, 1_700_000_000, body);

        // Valid.
        assert!(verify_signature(secret, &header, body).is_ok());

        // Tampered body.
        let err = verify_signature(secret, &header, br#"{"k":"x"}"#).unwrap_err();
        assert!(matches!(err, PlatformError::WebhookDelivery { .. }));

        // Wrong secret.
        assert!(verify_signature(b"other", &header, body).is_err());
    }

    #[test]
    fn verify_rejects_malformed_header() {
        let err = verify_signature(b"k", "totally-bogus", b"{}").unwrap_err();
        assert!(matches!(err, PlatformError::WebhookSigning { .. }));
    }

    #[test]
    fn endpoint_accepts_respects_subscriptions_and_active() {
        let ep = WebhookEndpoint::new("https://x/y", "s").subscribe("invoice.paid");
        assert!(ep.accepts("invoice.paid"));
        assert!(!ep.accepts("invoice.voided"));

        let receive_all = WebhookEndpoint::new("https://x/y", "s");
        assert!(receive_all.accepts("anything"));

        let inactive = WebhookEndpoint::new("https://x/y", "s").with_active(false);
        assert!(!inactive.accepts("anything"));
    }

    #[test]
    fn endpoint_debug_redacts_secret() {
        let ep = WebhookEndpoint::new("https://x/y", "super-secret");
        let dbg = format!("{ep:?}");
        assert!(!dbg.contains("super-secret"));
        assert!(dbg.contains("<redacted>"));
    }

    #[tokio::test]
    async fn recording_sender_captures_delivery_with_valid_signature() {
        let sender = RecordingWebhookSender::new();
        assert!(sender.is_empty());

        let ep = WebhookEndpoint::new("https://hooks.example/abc", "whsec_xyz")
            .subscribe("invoice.paid");
        let ev = event();

        sender.deliver(&ep, &ev).await.unwrap();

        let deliveries = sender.deliveries();
        assert_eq!(sender.len(), 1);
        let d = &deliveries[0];
        assert_eq!(d.endpoint_id, ep.id());
        assert_eq!(d.url, "https://hooks.example/abc");
        assert_eq!(d.event, ev);

        // The recorded signature verifies against the recorded body + secret.
        verify_signature(ep.secret().as_bytes(), &d.signature, d.body.as_bytes()).unwrap();
        // Timestamp in the header matches the event's occurred_at (seconds).
        assert!(d.signature.starts_with("t=1700000000,v1="));
    }

    #[tokio::test]
    async fn recording_sender_rejects_inactive_endpoint() {
        let sender = RecordingWebhookSender::new();
        let ep = WebhookEndpoint::new("https://x/y", "s").with_active(false);
        let err = sender.deliver(&ep, &event()).await.unwrap_err();
        assert!(matches!(err, PlatformError::WebhookDelivery { .. }));
        assert!(sender.is_empty());
    }

    #[test]
    fn event_round_trips_through_json() {
        let ev = event();
        let json = serde_json::to_string(&ev).unwrap();
        let back: WebhookEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(ev, back);
    }
}
