//! Named `{placeholder}` interpolation.

use std::collections::BTreeMap;
use std::fmt::Display;

/// Named arguments substituted into a message template.
///
/// ```
/// use klauthed_i18n::Args;
/// let args = Args::new().set("field", "email").set("min", 8);
/// assert_eq!(args.get("min"), Some("8"));
/// ```
#[derive(Debug, Clone, Default)]
pub struct Args {
    values: BTreeMap<String, String>,
}

impl Args {
    /// Empty arguments.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a named argument (builder form). Any `Display` value works.
    pub fn set(mut self, key: impl Into<String>, value: impl Display) -> Self {
        self.values.insert(key.into(), value.to_string());
        self
    }

    /// Look up a named argument.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }
}

impl<K: Into<String>, V: Display> FromIterator<(K, V)> for Args {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        let values = iter.into_iter().map(|(k, v)| (k.into(), v.to_string())).collect();
        Self { values }
    }
}

/// Substitute `{name}` placeholders in `template` using `args`.
///
/// * `{{` and `}}` are literal braces.
/// * A placeholder with no matching argument is left intact (so missing data is
///   visible rather than silently blank).
pub(crate) fn interpolate(template: &str, args: &Args) -> String {
    let mut out = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                out.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                out.push('}');
            }
            '{' => {
                let mut name = String::new();
                let mut closed = false;
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next == '}' {
                        closed = true;
                        break;
                    }
                    name.push(next);
                }
                match (closed, args.get(name.trim())) {
                    (true, Some(value)) => out.push_str(value),
                    // Unknown or unterminated: reproduce the original text.
                    (true, None) => {
                        out.push('{');
                        out.push_str(&name);
                        out.push('}');
                    }
                    (false, _) => {
                        out.push('{');
                        out.push_str(&name);
                    }
                }
            }
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitutes_named_args() {
        let args = Args::new().set("field", "email").set("min", 8);
        assert_eq!(
            interpolate("The field {field} needs {min} chars", &args),
            "The field email needs 8 chars"
        );
    }

    #[test]
    fn keeps_unknown_placeholders_and_escapes_braces() {
        let args = Args::new();
        assert_eq!(interpolate("{unknown}", &args), "{unknown}");
        assert_eq!(interpolate("literal {{x}} done", &args), "literal {x} done");
    }

    #[test]
    fn from_iter_builds_args() {
        let args = Args::from_iter([("a", "1"), ("b", "2")]);
        assert_eq!(args.get("a"), Some("1"));
        assert_eq!(args.get("b"), Some("2"));
    }
}
