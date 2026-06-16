//! W3C trace-context propagation across service boundaries (`otel` feature).
//!
//! The global propagator (W3C `traceparent` / `tracestate`) is installed by
//! [`crate::init`]. These helpers carry that context over HTTP: [`extract`] a
//! parent [`Context`] from an inbound request's headers (so a server span links
//! to its caller) and [`inject_current`] the active span's context into an
//! outbound request's headers (so the callee links back into this trace).

use opentelemetry::Context;
use opentelemetry::propagation::{Extractor, Injector};

/// Extract a parent [`Context`] from an inbound carrier via the global
/// propagator. Pair with an [`Extractor`] over your server's header type
/// (e.g. [`HeaderExtractor`] for the `http` crate's `HeaderMap`).
#[must_use]
pub fn extract<E: Extractor>(carrier: &E) -> Context {
    opentelemetry::global::get_text_map_propagator(|propagator| propagator.extract(carrier))
}

/// Inject `cx` into an outbound carrier as W3C trace-context headers.
pub fn inject<I: Injector>(cx: &Context, carrier: &mut I) {
    opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.inject_context(cx, carrier);
    });
}

/// Inject the **current** span's context into `headers` for an outbound HTTP
/// request, so the callee links into this trace.
///
/// ```no_run
/// # fn build(req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
/// let mut headers = http::HeaderMap::new();
/// klauthed_observability::propagation::inject_current(&mut headers);
/// req.headers(headers)
/// # }
/// ```
pub fn inject_current(headers: &mut http::HeaderMap) {
    use tracing_opentelemetry::OpenTelemetrySpanExt;
    let cx = tracing::Span::current().context();
    inject(&cx, &mut HeaderInjector(headers));
}

/// An [`Injector`] over the `http` crate's [`HeaderMap`](http::HeaderMap).
pub struct HeaderInjector<'a>(pub &'a mut http::HeaderMap);

impl Injector for HeaderInjector<'_> {
    fn set(&mut self, key: &str, value: String) {
        if let Ok(name) = http::header::HeaderName::from_bytes(key.as_bytes())
            && let Ok(val) = http::header::HeaderValue::from_str(&value)
        {
            self.0.insert(name, val);
        }
    }
}

/// An [`Extractor`] over the `http` crate's [`HeaderMap`](http::HeaderMap).
pub struct HeaderExtractor<'a>(pub &'a http::HeaderMap);

impl Extractor for HeaderExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|value| value.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(http::header::HeaderName::as_str).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry::trace::{
        SpanContext, SpanId, TraceContextExt, TraceFlags, TraceId, TraceState,
    };

    #[test]
    fn round_trips_trace_context_through_headers() {
        // `init` installs this in real use; set it directly for the unit test.
        opentelemetry::global::set_text_map_propagator(
            opentelemetry_sdk::propagation::TraceContextPropagator::new(),
        );

        let trace_id = TraceId::from_hex("0af7651916cd43dd8448eb211c80319c").unwrap();
        let span_id = SpanId::from_hex("b7ad6b7169203331").unwrap();
        let span_context =
            SpanContext::new(trace_id, span_id, TraceFlags::SAMPLED, true, TraceState::default());
        let cx = Context::new().with_remote_span_context(span_context);

        let mut headers = http::HeaderMap::new();
        inject(&cx, &mut HeaderInjector(&mut headers));

        // A W3C `traceparent` was written carrying our trace id.
        let traceparent = headers.get("traceparent").unwrap().to_str().unwrap();
        assert!(traceparent.contains("0af7651916cd43dd8448eb211c80319c"), "{traceparent}");

        // Extracting it back recovers the same trace.
        let extracted = extract(&HeaderExtractor(&headers));
        assert_eq!(extracted.span().span_context().trace_id(), trace_id);
    }
}
