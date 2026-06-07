//! Tests for the webhook model, signing, and senders.

use klauthed_core::time::Timestamp;
use klauthed_platform::PlatformError;

use klauthed_platform::webhooks::*;

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

    let ep =
        WebhookEndpoint::new("https://hooks.example/abc", "whsec_xyz").subscribe("invoice.paid");
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
