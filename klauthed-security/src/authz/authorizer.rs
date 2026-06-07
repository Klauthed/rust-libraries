//! The stateless [`Authorizer`] RBAC policy checker.

use crate::error::SecurityError;

use super::Permission;

/// The RBAC policy checker.
///
/// Stateless; all methods are associated functions operating on a caller-owned
/// set of granted permissions.
pub struct Authorizer;

impl Authorizer {
    /// Whether any permission in `granted` covers `required` (honouring
    /// wildcards via [`Permission::grants`]).
    #[must_use]
    pub fn is_authorized<'a, I>(granted: I, required: &Permission) -> bool
    where
        I: IntoIterator<Item = &'a Permission>,
    {
        granted.into_iter().any(|g| g.grants(required))
    }

    /// Like [`is_authorized`](Authorizer::is_authorized) but returns
    /// [`SecurityError::Forbidden`] (a `forbidden` domain error) instead of
    /// `false`, for use at call sites that propagate `Result`.
    ///
    /// # Errors
    /// [`SecurityError::Forbidden`] when no grant covers `required`.
    pub fn authorize<'a, I>(granted: I, required: &Permission) -> Result<(), SecurityError>
    where
        I: IntoIterator<Item = &'a Permission>,
    {
        if Self::is_authorized(granted, required) { Ok(()) } else { Err(SecurityError::Forbidden) }
    }
}
