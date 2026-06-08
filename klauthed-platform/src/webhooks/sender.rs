//! Webhook delivery: the [`WebhookSender`] trait, the in-memory
//! [`RecordingWebhookSender`], and the feature-gated `HttpWebhookSender`.

use std::sync::Mutex;

use async_trait::async_trait;

use crate::error::PlatformError;

use super::{WebhookEndpoint, WebhookEndpointId, WebhookEvent, sign_payload};

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
        self.deliveries.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
    }

    /// The number of recorded deliveries.
    pub fn len(&self) -> usize {
        self.deliveries.lock().unwrap_or_else(std::sync::PoisonError::into_inner).len()
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

        self.deliveries.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(
            WebhookDelivery {
                endpoint_id: endpoint.id(),
                url: endpoint.url().to_owned(),
                event: event.clone(),
                body,
                signature,
            },
        );
        Ok(())
    }
}

/// A [`WebhookSender`] that delivers events over HTTPS using `reqwest`.
///
/// Signs each delivery with HMAC-SHA256 (same as [`RecordingWebhookSender`])
/// and sets the signature in the `X-Klauthed-Signature` header. Retries are
/// left to the caller (e.g. via the job queue); this sender makes exactly one
/// HTTP attempt per call.
///
/// The request body is the JSON-serialised [`WebhookEvent`]. The endpoint URL
/// and secret come from the [`WebhookEndpoint`].
#[cfg(feature = "webhook-http")]
pub struct HttpWebhookSender {
    client: reqwest::Client,
    /// Header name used for the HMAC signature. Default: `"X-Klauthed-Signature"`.
    signature_header: &'static str,
    /// Request timeout per delivery attempt.
    timeout: std::time::Duration,
}

#[cfg(feature = "webhook-http")]
impl HttpWebhookSender {
    /// Build with default settings (30s timeout, `X-Klauthed-Signature` header).
    pub fn new() -> Result<Self, PlatformError> {
        let timeout = std::time::Duration::from_secs(30);
        let client = reqwest::Client::builder().timeout(timeout).build().map_err(|e| {
            PlatformError::WebhookDelivery { message: format!("build reqwest client: {e}") }
        })?;
        Ok(Self { client, signature_header: "X-Klauthed-Signature", timeout })
    }

    /// Override the request timeout.
    pub fn with_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Override the signature header name.
    pub fn with_signature_header(mut self, header: &'static str) -> Self {
        self.signature_header = header;
        self
    }
}

#[cfg(feature = "webhook-http")]
#[async_trait]
impl WebhookSender for HttpWebhookSender {
    async fn deliver(
        &self,
        endpoint: &WebhookEndpoint,
        event: &WebhookEvent,
    ) -> Result<(), PlatformError> {
        // 1. Serialise event to JSON bytes.
        let body_bytes = serde_json::to_vec(event).map_err(|e| PlatformError::WebhookSigning {
            message: format!("serialize event: {e}"),
        })?;

        // 2. Compute HMAC-SHA256 signature.
        let timestamp_secs = event.occurred_at().unix_seconds();
        let signature = sign_payload(endpoint.secret().as_bytes(), timestamp_secs, &body_bytes);

        // 3. POST with Content-Type and signature header.
        let response = self
            .client
            .post(endpoint.url())
            .header("Content-Type", "application/json")
            .header(self.signature_header, &signature)
            .body(body_bytes)
            .send()
            .await
            .map_err(|e| PlatformError::WebhookDelivery {
                message: format!("HTTP request to '{}': {e}", endpoint.url()),
            })?;

        // 4. Non-2xx → error.
        if !response.status().is_success() {
            return Err(PlatformError::WebhookDelivery {
                message: format!(
                    "endpoint '{}' returned HTTP {}",
                    endpoint.url(),
                    response.status()
                ),
            });
        }

        Ok(())
    }
}
