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

    /// Resource-instance-scoped authorization: whether `principal` may perform
    /// `required` on a resource owned by `owner`.
    ///
    /// Permitted if **either**:
    /// * a grant covers `required` outright — the principal may act on *any*
    ///   instance (e.g. an admin holding `articles:edit` or `articles:*`); **or**
    /// * `principal == owner` *and* a grant covers the `:own`-suffixed form
    ///   (`articles:edit:own`) — the principal may act only on instances they own.
    ///
    /// This bridges RBAC with per-instance scoping using the `:own` permission
    /// convention, so "edit any" and "edit your own" are distinct grants.
    #[must_use]
    pub fn is_authorized_for_resource<'a, I>(
        granted: I,
        required: &Permission,
        principal: &str,
        owner: &str,
    ) -> bool
    where
        I: IntoIterator<Item = &'a Permission>,
    {
        // Collect once so we can evaluate both the global and the `:own` grant.
        let granted: Vec<&Permission> = granted.into_iter().collect();
        if Self::is_authorized(granted.iter().copied(), required) {
            return true;
        }
        if principal == owner {
            let scoped = Permission::new(format!("{}:own", required.as_str()));
            return Self::is_authorized(granted.iter().copied(), &scoped);
        }
        false
    }

    /// Like [`is_authorized_for_resource`](Authorizer::is_authorized_for_resource)
    /// but returns [`SecurityError::Forbidden`] instead of `false`.
    ///
    /// # Errors
    /// [`SecurityError::Forbidden`] when the principal may act on neither any
    /// instance nor this owned one.
    pub fn authorize_for_resource<'a, I>(
        granted: I,
        required: &Permission,
        principal: &str,
        owner: &str,
    ) -> Result<(), SecurityError>
    where
        I: IntoIterator<Item = &'a Permission>,
    {
        if Self::is_authorized_for_resource(granted, required, principal, owner) {
            Ok(())
        } else {
            Err(SecurityError::Forbidden)
        }
    }
}
