//! The [`DiscoveryError`] type.

use klauthed_macros::DomainError;

/// Errors raised while registering with or querying a service registry.
///
/// The `DomainError` impl (category + `discovery.<reason>` code) is derived.
/// Most failures are transient backend outages (`unavailable`, retryable); a
/// rejected request or undecodable response is `internal`, and an empty lookup
/// is `not_found`.
#[derive(Debug, DomainError)]
#[domain(prefix = "discovery", category = "unavailable")]
#[non_exhaustive]
pub enum DiscoveryError {
    /// The registry backend was unreachable or returned a transport error.
    #[domain(category = "unavailable", code = "backend")]
    Backend(String),

    /// The backend rejected a register / deregister / heartbeat request.
    #[domain(category = "internal", code = "registration")]
    Registration(String),

    /// No instance was found for the requested service name.
    #[domain(category = "not_found", code = "no_instances")]
    NoInstances(String),

    /// A backend response could not be decoded.
    #[domain(category = "internal", code = "decode")]
    Decode(String),
}

impl std::fmt::Display for DiscoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiscoveryError::Backend(msg) => write!(f, "discovery backend error: {msg}"),
            DiscoveryError::Registration(msg) => write!(f, "service registration failed: {msg}"),
            DiscoveryError::NoInstances(service) => {
                write!(f, "no instances registered for service '{service}'")
            }
            DiscoveryError::Decode(msg) => write!(f, "could not decode registry response: {msg}"),
        }
    }
}

impl std::error::Error for DiscoveryError {}
