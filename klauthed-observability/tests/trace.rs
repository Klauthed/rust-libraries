//! Public-API integration tests for context-aware tracing spans.

use klauthed_core::context::RequestContext;
use klauthed_observability::{RecordContext, request_span};
use tracing::field::Empty;
use tracing::subscriber::with_default;
use tracing_subscriber::Registry;
use tracing_subscriber::prelude::*;

/// A lightweight, local subscriber so spans are enabled without touching the
/// global one (the crate's `init` must never run in tests).
fn local_subscriber() -> impl tracing::Subscriber + Send + Sync {
    Registry::default().with(tracing_subscriber::fmt::layer().with_test_writer())
}

#[test]
fn request_span_is_enabled_under_a_subscriber() {
    with_default(local_subscriber(), || {
        let ctx = RequestContext::new()
            .with_correlation_id("trace-abc")
            .with_tenant("acme")
            .with_principal("user-42")
            .with_locale("tr-TR");
        let span = request_span(&ctx);
        assert!(!span.is_disabled());
        // Entering and emitting an event must not panic.
        let _entered = span.enter();
        tracing::info!("inside request span");
    });
}

#[test]
fn request_span_with_minimal_context_does_not_panic() {
    with_default(local_subscriber(), || {
        let ctx = RequestContext::new();
        let span = request_span(&ctx);
        assert!(!span.is_disabled());
    });
}

#[test]
fn record_context_on_existing_span_does_not_panic() {
    with_default(local_subscriber(), || {
        let ctx = RequestContext::new().with_tenant("acme");
        let span = tracing::info_span!(
            "work",
            request_id = Empty,
            correlation_id = Empty,
            tenant = Empty,
            principal = Empty,
            locale = Empty,
        );
        span.record_context(&ctx);
        // Recording fields the span never declared is also a safe no-op.
        let bare = tracing::info_span!("bare");
        bare.record_context(&ctx);
    });
}

#[test]
fn helpers_work_without_any_subscriber() {
    // No subscriber installed: spans are disabled but nothing panics.
    let ctx = RequestContext::new().with_tenant("acme");
    let span = request_span(&ctx);
    span.record_context(&ctx);
    let _entered = span.enter();
}
