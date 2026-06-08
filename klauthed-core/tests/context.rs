//! Public-API integration tests for RequestContext: builders, deadlines, serde,
//! and (feature `task-local`) ambient propagation.

use klauthed_core::context::RequestContext;
use klauthed_core::time::Timestamp;

#[test]
fn builds_with_fields_and_unique_request_id() {
    let a = RequestContext::new();
    let b = RequestContext::new();
    assert_ne!(a.request_id(), b.request_id());

    let ctx = RequestContext::new()
        .with_correlation_id("corr-1")
        .with_tenant("acme")
        .with_principal("user-42")
        .with_locale("tr-TR")
        .with_metadata("k", "v");

    assert_eq!(ctx.correlation_id(), Some("corr-1"));
    assert_eq!(ctx.tenant(), Some("acme"));
    assert_eq!(ctx.principal(), Some("user-42"));
    assert_eq!(ctx.locale(), Some("tr-TR"));
    assert_eq!(ctx.metadata_get("k"), Some("v"));
}

#[test]
fn deadline_helpers() {
    let start = Timestamp::from_unix_millis(10_000);
    let ctx = RequestContext::new()
        .with_received_at(start)
        .with_deadline(Timestamp::from_unix_millis(15_000));

    let before = Timestamp::from_unix_millis(12_000);
    let after = Timestamp::from_unix_millis(16_000);

    assert!(!ctx.is_expired(before));
    assert!(ctx.is_expired(after));
    assert_eq!(ctx.time_remaining(before).unwrap().whole_seconds(), 3);
    assert_eq!(ctx.age(before).whole_seconds(), 2);
}

#[test]
fn serde_round_trip_skips_empty_fields() {
    let ctx = RequestContext::new().with_tenant("acme");
    let json = serde_json::to_string(&ctx).unwrap();
    assert!(json.contains("\"tenant\":\"acme\""));
    assert!(!json.contains("correlation_id"));
    let back: RequestContext = serde_json::from_str(&json).unwrap();
    assert_eq!(back.request_id(), ctx.request_id());
    assert_eq!(back.tenant(), Some("acme"));
}

#[cfg(feature = "task-local")]
#[tokio::test]
async fn ambient_context_is_readable_within_scope() {
    assert!(RequestContext::try_current().is_none());

    let ctx = RequestContext::new().with_tenant("acme");
    let id = ctx.request_id();

    ctx.scope(async move {
        let current = RequestContext::try_current().expect("context in scope");
        assert_eq!(current.request_id(), id);
        assert_eq!(current.tenant(), Some("acme"));
        let tenant = RequestContext::with_current(|c| c.tenant().map(str::to_owned)).flatten();
        assert_eq!(tenant.as_deref(), Some("acme"));
    })
    .await;

    assert!(RequestContext::try_current().is_none());
}
