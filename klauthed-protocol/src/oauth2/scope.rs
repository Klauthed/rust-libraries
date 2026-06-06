//! Scope-string helpers for the space-delimited `scope` wire form (RFC 6749 §3.3).

/// Join a list of scope tokens into the space-delimited wire form
/// (RFC 6749 section 3.3).
pub fn scope_to_string<I, S>(scopes: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut out = String::new();
    for s in scopes {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(s.as_ref());
    }
    out
}

/// Split the space-delimited `scope` wire form into individual tokens,
/// dropping empty segments (RFC 6749 section 3.3).
pub fn scope_from_str(scope: &str) -> Vec<String> {
    scope.split(' ').filter(|s| !s.is_empty()).map(str::to_owned).collect()
}
