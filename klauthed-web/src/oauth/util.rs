//! Internal redirect and error-response helpers for the OAuth handlers.

use actix_web::HttpResponse;
use actix_web::http::StatusCode;
use klauthed_protocol::oauth2::{OAuth2ErrorCode, TokenErrorResponse};

// ── URL construction ──────────────────────────────────────────────────────────

/// Append `key=percent_encoded_value` pairs to `base`, using `?` before the
/// first pair and `&` before subsequent ones.
pub(super) fn redirect_url(base: &str, params: &[(&str, &str)]) -> String {
    let mut url = base.to_owned();
    for (i, (k, v)) in params.iter().enumerate() {
        url.push(if i == 0 { '?' } else { '&' });
        url.push_str(k);
        url.push('=');
        url.push_str(&percent_encode(v));
    }
    url
}

/// Percent-encode a value for use in a URL parameter.
///
/// Passes through unreserved characters (RFC 3986 §2.3); encodes everything
/// else as `%XX`. This keeps OAuth codes, states, and error descriptions
/// safe inside redirect URIs.
pub(super) fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

// ── Responses ─────────────────────────────────────────────────────────────────

/// `302 Found` redirect to `location`.
pub(super) fn redirect(location: &str) -> HttpResponse {
    HttpResponse::Found().insert_header(("Location", location)).finish()
}

/// Redirect to `redirect_uri` carrying an OAuth error code and optional state.
///
/// Used by the `/authorize` endpoint where errors must be communicated via
/// the client's registered `redirect_uri`.
pub(super) fn error_redirect(
    redirect_uri: &str,
    error: OAuth2ErrorCode,
    description: &str,
    state: Option<&str>,
) -> HttpResponse {
    let error_str = serde_json::to_value(error)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "server_error".into());

    let mut params: Vec<(&str, &str)> =
        vec![("error", &error_str), ("error_description", description)];
    let state_owned;
    if let Some(s) = state {
        state_owned = s.to_owned();
        params.push(("state", &state_owned));
    }
    redirect(&redirect_url(redirect_uri, &params))
}

/// JSON error response for the `/token` endpoint (RFC 6749 §5.2).
///
/// Always `400 Bad Request` per the spec.
pub(super) fn token_error(code: OAuth2ErrorCode, description: &str) -> HttpResponse {
    HttpResponse::build(StatusCode::BAD_REQUEST)
        .json(TokenErrorResponse::with_description(code, description))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redirect_url_encodes_params() {
        let url =
            redirect_url("https://app.example.com/cb", &[("code", "abc123"), ("state", "x y")]);
        assert_eq!(url, "https://app.example.com/cb?code=abc123&state=x%20y");
    }

    #[test]
    fn percent_encode_passes_unreserved_chars() {
        assert_eq!(percent_encode("abc-_123.~"), "abc-_123.~");
        assert_eq!(percent_encode("a b"), "a%20b");
        assert_eq!(percent_encode("a:b/c"), "a%3Ab%2Fc");
    }
}
