//! Tests for the audit event model and sinks.

use klauthed_core::context::RequestContext;

use super::*;

#[test]
fn builder_defaults_and_overrides() {
    let e = AuditEvent::builder("login")
        .actor("user-1")
        .tenant("acme")
        .resource("session", "s-9")
        .metadata("ip", "127.0.0.1")
        .build();

    assert_eq!(e.action(), "login");
    assert_eq!(e.actor(), Some("user-1"));
    assert_eq!(e.tenant(), Some("acme"));
    assert_eq!(e.resource_type(), Some("session"));
    assert_eq!(e.resource_id(), Some("s-9"));
    assert_eq!(e.outcome(), AuditOutcome::Success);
    assert_eq!(e.metadata().get("ip").map(String::as_str), Some("127.0.0.1"));
}

#[test]
fn failed_sets_failure_outcome() {
    let e = AuditEvent::builder("delete").failed().build();
    assert_eq!(e.outcome(), AuditOutcome::Failure);
    assert!(!e.outcome().is_success());
}

#[test]
fn from_context_fills_actor_tenant_and_request_id() {
    let ctx = RequestContext::new().with_principal("p-7").with_tenant("acme");
    let e = AuditEvent::builder("read").from_context(&ctx).build();
    assert_eq!(e.actor(), Some("p-7"));
    assert_eq!(e.tenant(), Some("acme"));
    assert_eq!(
        e.metadata().get("request_id").map(String::as_str),
        Some(ctx.request_id().to_string().as_str())
    );
}

#[test]
fn from_context_does_not_overwrite_explicit_values() {
    let ctx = RequestContext::new().with_principal("ctx-actor");
    let e = AuditEvent::builder("x").actor("explicit").from_context(&ctx).build();
    assert_eq!(e.actor(), Some("explicit"));
}

#[test]
fn event_round_trips_through_json() {
    let e = AuditEvent::builder("x").actor("a").metadata("k", "v").build();
    let json = serde_json::to_string(&e).unwrap();
    let back: AuditEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(e, back);
}

#[tokio::test]
async fn in_memory_sink_retains_events() {
    let sink = InMemoryAuditSink::new();
    assert!(sink.is_empty());

    sink.record(AuditEvent::builder("a").build()).await.unwrap();
    sink.record(AuditEvent::builder("b").build()).await.unwrap();

    let events = sink.events();
    assert_eq!(sink.len(), 2);
    assert_eq!(events[0].action(), "a");
    assert_eq!(events[1].action(), "b");
}

// ── OutboxAuditSink tests (require `audit-outbox` feature + SQLite) ───────

#[cfg(feature = "audit-outbox")]
mod outbox_sink_tests {
    use super::*;
    use klauthed_data::outbox::sql::SqlOutbox;
    use klauthed_data::outbox::{Outbox, OutboxEntry};

    async fn memory_outbox() -> SqlOutbox {
        sqlx::any::install_default_drivers();
        let pool = sqlx::pool::PoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect in-memory sqlite");
        let outbox = SqlOutbox::new(pool);
        outbox.ensure_schema().await.expect("ensure schema");
        outbox
    }

    #[tokio::test]
    async fn record_inserts_one_entry_into_outbox() {
        let outbox = memory_outbox().await;
        let sink = OutboxAuditSink::new(outbox.clone());

        let event = AuditEvent::builder("tenant.suspend")
            .actor("admin-1")
            .tenant("acme")
            .outcome(AuditOutcome::Success)
            .build();

        sink.record(event).await.unwrap();

        let entries: Vec<OutboxEntry> = outbox.fetch_unpublished(10).await.unwrap();
        assert_eq!(entries.len(), 1);
    }

    #[tokio::test]
    async fn record_payload_round_trips_as_audit_event() {
        let outbox = memory_outbox().await;
        let sink = OutboxAuditSink::new(outbox.clone());

        let original = AuditEvent::builder("user.login")
            .actor("u-99")
            .tenant("beta")
            .metadata("ip", "10.0.0.1")
            .build();

        sink.record(original.clone()).await.unwrap();

        let entries: Vec<OutboxEntry> = outbox.fetch_unpublished(10).await.unwrap();
        assert_eq!(entries.len(), 1);

        let recovered: AuditEvent = serde_json::from_value(entries[0].payload.clone()).unwrap();
        assert_eq!(recovered, original);
    }

    #[tokio::test]
    async fn record_sets_aggregate_type_and_event_type() {
        let outbox = memory_outbox().await;
        let sink = OutboxAuditSink::new(outbox.clone());

        let event = AuditEvent::builder("invoice.paid").actor("svc-billing").build();
        let action = event.action().to_owned();

        sink.record(event).await.unwrap();

        let entries: Vec<OutboxEntry> = outbox.fetch_unpublished(10).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].aggregate_type, "audit");
        assert_eq!(entries[0].event_type, action);
    }
}
