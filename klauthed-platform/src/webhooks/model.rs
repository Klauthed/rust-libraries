//! Webhook model types: [`WebhookEndpoint`] and [`WebhookEvent`] (with their
//! typed ids).

use klauthed_core::id::Id;
use klauthed_core::time::Timestamp;
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
    #[must_use]
    pub fn with_id(mut self, id: WebhookEndpointId) -> Self {
        self.id = id;
        self
    }

    /// Subscribe to an event type (builder style; duplicates are ignored).
    #[must_use]
    pub fn subscribe(mut self, event_type: impl Into<String>) -> Self {
        let ty = event_type.into();
        if !self.event_types.contains(&ty) {
            self.event_types.push(ty);
        }
        self
    }

    /// Set the active flag (builder style).
    #[must_use]
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
            && (self.event_types.is_empty() || self.event_types.iter().any(|t| t == event_type))
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
        Self { id: WebhookEventId::new(), event_type: event_type.into(), occurred_at, data }
    }

    /// Override the event id (builder style).
    #[must_use]
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
        serde_json::to_string(self)
            .map_err(|e| PlatformError::WebhookSigning { message: format!("serialize event: {e}") })
    }
}
