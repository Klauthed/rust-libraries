use std::borrow::Cow;
use std::fmt;

/// A stable, machine-readable error code for logs and API responses.
///
/// Codes follow a `domain.reason` convention (e.g. `config.missing_required`,
/// `data.unavailable`). They are usually `&'static str` constants, but a dynamic
/// `String` is supported for codes assembled at runtime.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct ErrorCode(Cow<'static, str>);

impl ErrorCode {
    /// A code from a static string — the common case.
    pub const fn new(code: &'static str) -> Self {
        ErrorCode(Cow::Borrowed(code))
    }

    /// A code assembled at runtime.
    pub fn from_string(code: String) -> Self {
        ErrorCode(Cow::Owned(code))
    }

    /// The code as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&'static str> for ErrorCode {
    fn from(code: &'static str) -> Self {
        ErrorCode::new(code)
    }
}

impl From<String> for ErrorCode {
    fn from(code: String) -> Self {
        ErrorCode::from_string(code)
    }
}
