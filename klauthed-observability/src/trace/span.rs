//! The [`request_span`] constructor and the shared optional-field recorder.

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

/// Record the optional context fields that are set. Recording a field the span
/// never declared is ignored by `tracing`, so callers needn't pre-declare all
/// of them.
pub(crate) fn record_optional_fields(span: &tracing::Span, ctx: &RequestContext) {
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
