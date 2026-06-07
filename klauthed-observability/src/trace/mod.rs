//! Context-aware tracing spans.
//!
//! Bridges a [`RequestContext`] into the `tracing` world: [`request_span`]
//! mints a `request` span carrying the cross-cutting identifiers
//! (`request_id`, and, when present, `correlation_id`, `tenant`, `principal`,
//! `locale`), and [`RecordContext::record_context`] attaches those same fields
//! to an existing span.
//!
//! Entering the returned span makes its fields appear on every child span and
//! event emitted while it is active. With the JSON subscriber installed by
//! [`crate::init`] this means each log line is automatically tagged with the
//! request's identity — no manual threading required.
//!
//! Future work: automatic span creation from a tower/actix layer, and OTLP
//! exemplars correlating these spans with metrics, belong in the consuming
//! crate (e.g. `klauthed-web`), not here.
//!
//! ```
//! use klauthed_core::context::RequestContext;
//! use klauthed_observability::request_span;
//!
//! let ctx = RequestContext::new().with_tenant("acme");
//! let span = request_span(&ctx);
//! let _entered = span.enter();
//! tracing::info!("handling request"); // carries request_id + tenant
//! ```

use klauthed_core::context::RequestContext;
use tracing::field::Empty;

/// Create the root `request` span for `ctx`.
///
/// The span always carries `request_id`; `correlation_id`, `tenant`,
/// `principal`, and `locale` are recorded only when present on the context
/// (absent fields are declared as [`Empty`] so the subscriber omits them).
///
/// Enter the span (`let _g = span.enter();`) or instrument a future with it so
/// the fields propagate to all child spans and events.
///
/// ```
/// use klauthed_core::context::RequestContext;
/// use klauthed_observability::request_span;
///
/// let ctx = RequestContext::new()
///     .with_correlation_id("trace-abc")
///     .with_principal("user-42");
/// let span = request_span(&ctx);
/// let _entered = span.enter();
/// tracing::info!("processing"); // tagged with request_id, correlation_id, principal
/// ```
pub fn request_span(ctx: &RequestContext) -> tracing::Span {
    // Declare every optional field up front as `Empty`, then fill in the ones
    // that are actually present. This keeps the span's field set stable (a
    // `tracing` requirement: fields must be known at span creation) while
    // emitting only the values that exist.
    let span = tracing::info_span!(
        "request",
        request_id = %ctx.request_id(),
        correlation_id = Empty,
        tenant = Empty,
        principal = Empty,
        locale = Empty,
    );
    record_optional_fields(&span, ctx);
    span
}

/// Attach `ctx`'s identifying fields to an existing span.
///
/// This is the extension-trait form of the recording logic used by
/// [`request_span`]. Use it when you already hold a span (for example one
/// created by `#[tracing::instrument]`) that declared the relevant fields as
/// [`Empty`], and want to populate them from a context obtained later.
///
/// Recording a field the span did not declare is a no-op in `tracing`, so this
/// is always safe to call.
///
/// ```
/// use klauthed_core::context::RequestContext;
/// use klauthed_observability::RecordContext;
///
/// let ctx = RequestContext::new().with_tenant("acme");
/// let span = tracing::info_span!(
///     "work",
///     request_id = tracing::field::Empty,
///     tenant = tracing::field::Empty,
/// );
/// span.record_context(&ctx);
/// ```
pub trait RecordContext {
    /// Record `ctx`'s fields onto `self`.
    fn record_context(&self, ctx: &RequestContext);
}

impl RecordContext for tracing::Span {
    fn record_context(&self, ctx: &RequestContext) {
        // `request_id` is always present; the rest are conditional.
        self.record("request_id", tracing::field::display(ctx.request_id()));
        record_optional_fields(self, ctx);
    }
}

/// Record the optional context fields that are set. Recording a field the span
/// never declared is ignored by `tracing`, so callers needn't pre-declare all
/// of them.
fn record_optional_fields(span: &tracing::Span, ctx: &RequestContext) {
    if let Some(correlation_id) = ctx.correlation_id() {
        span.record("correlation_id", correlation_id);
    }
    if let Some(tenant) = ctx.tenant() {
        span.record("tenant", tenant);
    }
    if let Some(principal) = ctx.principal() {
        span.record("principal", principal);
    }
    if let Some(locale) = ctx.locale() {
        span.record("locale", locale);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
