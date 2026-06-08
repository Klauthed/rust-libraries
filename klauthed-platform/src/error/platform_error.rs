use klauthed_macros::DomainError;

/// Errors raised by platform operations (tenancy, feature flags, audit).
///
/// ```
/// use klauthed_error::{DomainError, ErrorCategory};
/// use klauthed_platform::PlatformError;
///
/// let err = PlatformError::TenantNotFound { id_or_slug: "acme".into() };
/// assert_eq!(err.category(), ErrorCategory::NotFound);
/// assert_eq!(err.code().as_str(), "platform.tenant_not_found");
/// assert_eq!(err.http_status(), 404);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, DomainError)]
#[domain(prefix = "platform", category = "internal")]
#[non_exhaustive]
pub enum PlatformError {
    /// No tenant matched the given id or slug.
    #[domain(category = "not_found")]
    TenantNotFound {
        /// The id or slug that was looked up.
        id_or_slug: String,
    },

    /// The tenant exists but is not [`Active`](crate::TenantStatus::Active).
    #[domain(category = "forbidden")]
    TenantSuspended {
        /// The slug of the offending tenant.
        slug: String,
    },

    /// A tenant slug was empty or otherwise malformed.
    #[domain(category = "bad_request")]
    InvalidTenantSlug {
        /// The rejected slug value.
        slug: String,
    },

    /// No job matched the given [`JobId`](crate::jobs::JobId).
    #[domain(category = "not_found")]
    JobNotFound {
        /// The job id that was looked up.
        id: String,
    },

    /// Computing or formatting a webhook HMAC signature failed.
    #[domain(category = "internal")]
    WebhookSigning {
        /// Human-readable cause.
        message: String,
    },

    /// Delivering a webhook to its endpoint failed (e.g. transport error).
    #[domain(category = "unavailable")]
    WebhookDelivery {
        /// Human-readable cause.
        message: String,
    },

    /// A backing store (resolver / sink) failed for an unexpected reason.
    #[domain(category = "internal")]
    Backend {
        /// Human-readable cause.
        message: String,
    },
}

impl std::fmt::Display for PlatformError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlatformError::TenantNotFound { id_or_slug } => {
                write!(f, "no tenant found for '{id_or_slug}'")
            }
            PlatformError::TenantSuspended { slug } => {
                write!(f, "tenant '{slug}' is not active")
            }
            PlatformError::InvalidTenantSlug { slug } => {
                write!(f, "invalid tenant slug '{slug}'")
            }
            PlatformError::JobNotFound { id } => {
                write!(f, "no job found for '{id}'")
            }
            PlatformError::WebhookSigning { message } => {
                write!(f, "webhook signing failed: {message}")
            }
            PlatformError::WebhookDelivery { message } => {
                write!(f, "webhook delivery failed: {message}")
            }
            PlatformError::Backend { message } => {
                write!(f, "platform backend error: {message}")
            }
        }
    }
}

impl std::error::Error for PlatformError {}
