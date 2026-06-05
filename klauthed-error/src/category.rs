use std::fmt;

/// A coarse classification every klauthed error maps onto.
///
/// The category is the single source of truth for cross-cutting policy — HTTP
/// status, retryability, client-vs-server — so individual error types only have
/// to answer "which category am I?" rather than re-deriving an HTTP code each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ErrorCategory {
    /// Caller sent something invalid (validation, malformed input). `400`.
    BadRequest,
    /// Authentication is missing or invalid. `401`.
    Unauthorized,
    /// Authenticated but not allowed. `403`.
    Forbidden,
    /// The target resource does not exist. `404`.
    NotFound,
    /// The request conflicts with current state. `409`.
    Conflict,
    /// The caller has been rate limited. `429`.
    RateLimited,
    /// The operation timed out. `504`.
    Timeout,
    /// A dependency (DB, cache, broker, upstream) is unavailable. `503`.
    Unavailable,
    /// An unexpected server-side fault. `500`.
    Internal,
}

impl ErrorCategory {
    /// The conventional HTTP status for this category.
    pub fn http_status(&self) -> u16 {
        match self {
            ErrorCategory::BadRequest => 400,
            ErrorCategory::Unauthorized => 401,
            ErrorCategory::Forbidden => 403,
            ErrorCategory::NotFound => 404,
            ErrorCategory::Conflict => 409,
            ErrorCategory::RateLimited => 429,
            ErrorCategory::Timeout => 504,
            ErrorCategory::Unavailable => 503,
            ErrorCategory::Internal => 500,
        }
    }

    /// Whether retrying the operation might succeed (transient conditions).
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            ErrorCategory::RateLimited | ErrorCategory::Timeout | ErrorCategory::Unavailable
        )
    }

    /// Whether this represents a caller error (`4xx`) rather than a server fault.
    pub fn is_client_error(&self) -> bool {
        let status = self.http_status();
        (400..500).contains(&status)
    }

    /// A stable, lowercase label.
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorCategory::BadRequest => "bad_request",
            ErrorCategory::Unauthorized => "unauthorized",
            ErrorCategory::Forbidden => "forbidden",
            ErrorCategory::NotFound => "not_found",
            ErrorCategory::Conflict => "conflict",
            ErrorCategory::RateLimited => "rate_limited",
            ErrorCategory::Timeout => "timeout",
            ErrorCategory::Unavailable => "unavailable",
            ErrorCategory::Internal => "internal",
        }
    }
}

impl fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_and_retryability_align() {
        assert_eq!(ErrorCategory::NotFound.http_status(), 404);
        assert!(ErrorCategory::NotFound.is_client_error());
        assert!(!ErrorCategory::NotFound.is_retryable());

        assert_eq!(ErrorCategory::Unavailable.http_status(), 503);
        assert!(!ErrorCategory::Unavailable.is_client_error());
        assert!(ErrorCategory::Unavailable.is_retryable());

        assert!(!ErrorCategory::Internal.is_retryable());
    }
}
