//! The [`RecordContext`] extension trait for attaching context to a live span.

use klauthed_core::context::RequestContext;

use super::span::record_optional_fields;

/// Attach `ctx`'s identifying fields to an existing span.
///
/// This is the extension-trait form of the recording logic used by
/// [`request_span`](super::request_span). Use it when you already hold a span (for example one
/// created by `#[tracing::instrument]`) that declared the relevant fields as
/// [`Empty`](tracing::field::Empty), and want to populate them from a context obtained later.
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
