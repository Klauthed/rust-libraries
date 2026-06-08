//! Framework-agnostic recording helpers (HTTP request metrics + thin wrappers).

// ── HTTP server metrics ───────────────────────────────────────────────────────
//
// These are reusable, framework-agnostic helpers. Wiring them into a request
// pipeline (an actix-web middleware, a tower `Layer`) lives in the consuming
// crate (e.g. `klauthed-web`), not here. Future work: exemplars linking metrics
// to trace ids once the exporter supports them.

/// Counter name for completed HTTP server requests.
const HTTP_REQUESTS_TOTAL: &str = "http_requests_total";
/// Histogram name for HTTP server request latency, in seconds.
const HTTP_REQUEST_DURATION_SECONDS: &str = "http_request_duration_seconds";

/// Record one completed HTTP server request.
///
/// Emits the `http_requests_total` counter (incremented by one) and the
/// `http_request_duration_seconds` histogram, both labelled by `method`,
/// `path`, and `status`. The latency is recorded in fractional seconds, the
/// convention Prometheus tooling expects for a `_seconds` histogram.
///
/// If no recorder has been installed (see [`install`](super::install)) the underlying `metrics`
/// macros are cheap no-ops, so this is always safe to call.
///
/// # Label cardinality
///
/// `path` should be a low-cardinality *route template* (e.g. `/users/{id}`),
/// **not** the raw request path (`/users/12345`). Passing raw paths makes the
/// metric's cardinality unbounded and will overwhelm Prometheus. Normalize the
/// path to its matched route before calling this. Likewise prefer a small,
/// fixed set of `method` values.
///
/// ```
/// use std::time::Duration;
/// use klauthed_observability::metrics::record_http_request;
///
/// // Safe with or without a recorder installed.
/// record_http_request("GET", "/users/{id}", 200, Duration::from_millis(12));
/// ```
pub fn record_http_request(method: &str, path: &str, status: u16, latency: std::time::Duration) {
    let method = method.to_owned();
    let path = path.to_owned();
    let status = status.to_string();

    metrics::counter!(
        HTTP_REQUESTS_TOTAL,
        "method" => method.clone(),
        "path" => path.clone(),
        "status" => status.clone(),
    )
    .increment(1);

    metrics::histogram!(
        HTTP_REQUEST_DURATION_SECONDS,
        "method" => method,
        "path" => path,
        "status" => status,
    )
    .record(latency.as_secs_f64());
}

// ── Thin generic wrappers ─────────────────────────────────────────────────────

/// Increment a named counter by `value` with the given `labels`.
///
/// A thin, non-macro wrapper over [`metrics::counter!`] for cases where the
/// metric name or labels are computed at runtime. Like all `metrics` macros it
/// is a no-op when no recorder is installed.
///
/// ```
/// use klauthed_observability::metrics::inc_counter;
///
/// inc_counter("widgets_created_total", 1, &[("kind", "gadget")]);
/// ```
pub fn inc_counter(name: &'static str, value: u64, labels: &[(&'static str, &str)]) {
    let labels: Vec<metrics::Label> =
        labels.iter().map(|(k, v)| metrics::Label::new(*k, v.to_string())).collect();
    metrics::counter!(name, labels).increment(value);
}

/// Record `value` into a named histogram with the given `labels`.
///
/// A thin, non-macro wrapper over [`metrics::histogram!`]. No-op without a
/// recorder installed.
///
/// ```
/// use klauthed_observability::metrics::observe;
///
/// observe("payload_bytes", 2048.0, &[("route", "/upload")]);
/// ```
pub fn observe(name: &'static str, value: f64, labels: &[(&'static str, &str)]) {
    let labels: Vec<metrics::Label> =
        labels.iter().map(|(k, v)| metrics::Label::new(*k, v.to_string())).collect();
    metrics::histogram!(name, labels).record(value);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    // These run without a recorder installed; the macros are no-ops and must
    // not panic. We avoid installing the global recorder so tests stay
    // isolated and re-runnable.

    #[test]
    fn record_http_request_does_not_panic_without_recorder() {
        record_http_request("GET", "/users/{id}", 200, Duration::from_millis(7));
        record_http_request("POST", "/login", 401, Duration::from_micros(250));
    }

    #[test]
    fn thin_wrappers_do_not_panic_without_recorder() {
        inc_counter("test_counter_total", 3, &[("label", "value")]);
        observe("test_histogram", 1.5, &[]);
    }
}
