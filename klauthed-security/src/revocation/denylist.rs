//! The [`TokenDenylist`] async storage trait.

use async_trait::async_trait;

use klauthed_core::time::Timestamp;

use crate::error::SecurityError;

/// Async storage for revoked JWT `jti` values.
///
/// Implementations must be `Send + Sync` so they can be held as
/// `Arc<dyn TokenDenylist>` and shared across worker threads.
///
/// # Expiry contract
///
/// Callers should pass `expires_at` equal to the token's own `exp` claim (as a
/// [`Timestamp`]). The denylist uses this to know when the entry can be pruned
/// — once the token is naturally expired, the denylist entry is redundant.
#[async_trait]
pub trait TokenDenylist: Send + Sync {
    /// Mark `jti` as revoked, with the entry expiring at `expires_at`.
    ///
    /// Revoking an already-revoked `jti` is idempotent; the expiry is updated
    /// to the new value.
    ///
    /// # Errors
    /// Returns [`SecurityError`] on backend failure.
    async fn revoke(&self, jti: String, expires_at: Timestamp) -> Result<(), SecurityError>;

    /// Return `true` if `jti` is in the denylist and its entry has not yet
    /// expired.
    ///
    /// A `false` result means either the token was never revoked, or its
    /// denylist entry has expired (the token would also fail `exp` validation
    /// at the verifier, making the check redundant).
    ///
    /// # Errors
    /// Returns [`SecurityError`] on backend failure.
    async fn is_revoked(&self, jti: &str) -> Result<bool, SecurityError>;
}
