//! A small role-based access-control (RBAC) model.
//!
//! The pieces:
//!
//! * [`Permission`] — a stable, colon-namespaced action string like
//!   `"users:read"`. Supports wildcards: `"users:*"` grants every action in the
//!   `users` namespace, and `"*"` grants everything.
//! * [`Role`] — a named bag of [`Permission`]s.
//! * [`RoleRegistry`] — maps role names → roles, and resolves a principal's
//!   *effective* permissions from the roles it has been granted.
//! * [`Authorizer`] — the policy checker: does a set of granted permissions
//!   satisfy a required one ([`is_authorized`](Authorizer::is_authorized))?
//!
//! Matching is *grant*-side wildcard: a wildcard in a **granted** permission
//! widens what it covers; the **required** permission is normally concrete
//! (`"users:read"`). A required permission may itself be a wildcard, in which
//! case it matches only if some grant is at least as broad.
//!
//! ```
//! use klauthed_security::authz::{Authorizer, Permission, Role, RoleRegistry};
//!
//! let mut registry = RoleRegistry::new();
//! registry.define(Role::new("editor").with_permissions([
//!     Permission::new("articles:read"),
//!     Permission::new("articles:write"),
//! ]));
//! registry.define(Role::new("admin").with_permissions([Permission::new("*")]));
//!
//! let editor_perms = registry.effective_permissions(["editor"]);
//! assert!(Authorizer::is_authorized(&editor_perms, &Permission::new("articles:read")));
//! assert!(!Authorizer::is_authorized(&editor_perms, &Permission::new("users:delete")));
//!
//! // The admin's `*` matches anything.
//! let admin_perms = registry.effective_permissions(["admin"]);
//! assert!(Authorizer::is_authorized(&admin_perms, &Permission::new("users:delete")));
//! ```
//!
//! # Not (yet) included
//!
//! This is deliberately plain RBAC. Attribute-based access control (ABAC),
//! resource-instance scoping, role hierarchies/inheritance, and a general
//! policy engine (e.g. Cedar/OPA-style) are future work that would layer on top
//! of these types.

use std::collections::BTreeSet;

use crate::error::SecurityError;

/// A namespaced permission string, e.g. `"users:read"`.
///
/// By convention permissions are `"<resource>:<action>"`, but the type only
/// requires a string; matching treats `*` segments as wildcards. The two special
/// forms are `"*"` (everything) and `"<resource>:*"` (every action on a
/// resource).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Permission(String);

impl Permission {
    /// Wrap a permission string.
    pub fn new(perm: impl Into<String>) -> Self {
        Self(perm.into())
    }

    /// The permission as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether this (granted) permission covers `required`.
    ///
    /// Both sides are split on `:` into segments; a `*` segment in *this*
    /// permission matches any single segment in `required`, and a trailing `*`
    /// segment matches all remaining segments (so `"users:*"` covers
    /// `"users:read"`). A bare `"*"` covers everything.
    #[must_use]
    pub fn grants(&self, required: &Permission) -> bool {
        if self.0 == "*" {
            return true;
        }
        let granted: Vec<&str> = self.0.split(':').collect();
        let needed: Vec<&str> = required.0.split(':').collect();

        for (i, g) in granted.iter().enumerate() {
            // A trailing `*` segment swallows all remaining required segments,
            // but only if there is at least one to swallow (so `users:*` covers
            // `users:read` / `users:read:extra`, but not the bare `users`).
            if *g == "*" && i == granted.len() - 1 {
                return needed.len() > i;
            }
            match needed.get(i) {
                Some(n) if *g == "*" || g == n => {}
                _ => return false,
            }
        }
        // All granted segments matched; it's a grant only if there were no extra
        // required segments left over (i.e. equal length, exact match).
        granted.len() == needed.len()
    }
}

impl std::fmt::Display for Permission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for Permission {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for Permission {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// A named set of [`Permission`]s.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Role {
    /// The role's unique name (e.g. `"admin"`).
    pub name: String,
    /// The permissions granted by holding this role.
    pub permissions: BTreeSet<Permission>,
}

impl Role {
    /// A role named `name` with no permissions yet.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            permissions: BTreeSet::new(),
        }
    }

    /// Builder: add `permissions` to this role.
    #[must_use]
    pub fn with_permissions<I, P>(mut self, permissions: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<Permission>,
    {
        self.permissions
            .extend(permissions.into_iter().map(Into::into));
        self
    }

    /// Add a single permission in place.
    pub fn grant(&mut self, permission: impl Into<Permission>) {
        self.permissions.insert(permission.into());
    }
}

/// An in-memory registry mapping role names → [`Role`]s.
#[derive(Debug, Clone, Default)]
pub struct RoleRegistry {
    roles: std::collections::HashMap<String, Role>,
}

impl RoleRegistry {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Define (or replace) a role by its name.
    pub fn define(&mut self, role: Role) {
        self.roles.insert(role.name.clone(), role);
    }

    /// Look up a role by name.
    #[must_use]
    pub fn role(&self, name: &str) -> Option<&Role> {
        self.roles.get(name)
    }

    /// Resolve the effective (de-duplicated) permission set for a principal that
    /// holds the given role names. Unknown role names are ignored.
    pub fn effective_permissions<I, S>(&self, role_names: I) -> BTreeSet<Permission>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut perms = BTreeSet::new();
        for name in role_names {
            if let Some(role) = self.roles.get(name.as_ref()) {
                perms.extend(role.permissions.iter().cloned());
            }
        }
        perms
    }
}

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
        if Self::is_authorized(granted, required) {
            Ok(())
        } else {
            Err(SecurityError::Forbidden)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use klauthed_error::{DomainError, ErrorCategory};

    fn perms(list: &[&str]) -> Vec<Permission> {
        list.iter().map(|s| Permission::new(*s)).collect()
    }

    #[test]
    fn exact_match_allows() {
        let granted = perms(&["users:read"]);
        assert!(Authorizer::is_authorized(
            &granted,
            &Permission::new("users:read")
        ));
    }

    #[test]
    fn missing_permission_denies() {
        let granted = perms(&["users:read"]);
        assert!(!Authorizer::is_authorized(
            &granted,
            &Permission::new("users:write")
        ));
    }

    #[test]
    fn resource_wildcard_matches_actions() {
        let granted = perms(&["users:*"]);
        assert!(Authorizer::is_authorized(
            &granted,
            &Permission::new("users:read")
        ));
        assert!(Authorizer::is_authorized(
            &granted,
            &Permission::new("users:delete")
        ));
        // Different resource is not covered.
        assert!(!Authorizer::is_authorized(
            &granted,
            &Permission::new("orders:read")
        ));
    }

    #[test]
    fn global_wildcard_matches_everything() {
        let granted = perms(&["*"]);
        assert!(Authorizer::is_authorized(
            &granted,
            &Permission::new("anything:goes")
        ));
        assert!(Authorizer::is_authorized(
            &granted,
            &Permission::new("a:b:c")
        ));
    }

    #[test]
    fn mid_segment_wildcard_matches_one_segment() {
        let granted = perms(&["users:*:read"]);
        assert!(Authorizer::is_authorized(
            &granted,
            &Permission::new("users:42:read")
        ));
        // Length mismatch is not a grant.
        assert!(!Authorizer::is_authorized(
            &granted,
            &Permission::new("users:42")
        ));
    }

    #[test]
    fn resource_wildcard_does_not_grant_shorter_requirement() {
        // "users:*" should not grant the bare "users".
        let granted = perms(&["users:*"]);
        assert!(!Authorizer::is_authorized(
            &granted,
            &Permission::new("users")
        ));
    }

    #[test]
    fn registry_resolves_effective_permissions_from_roles() {
        let mut reg = RoleRegistry::new();
        reg.define(
            Role::new("reader").with_permissions([Permission::new("articles:read")]),
        );
        reg.define(Role::new("writer").with_permissions(["articles:write"]));

        let effective = reg.effective_permissions(["reader", "writer", "unknown"]);
        let collected: Vec<&Permission> = effective.iter().collect();
        assert!(Authorizer::is_authorized(
            collected.iter().copied(),
            &Permission::new("articles:read")
        ));
        assert!(Authorizer::is_authorized(
            effective.iter(),
            &Permission::new("articles:write")
        ));
        assert!(!Authorizer::is_authorized(
            effective.iter(),
            &Permission::new("articles:delete")
        ));
    }

    #[test]
    fn admin_role_with_global_wildcard() {
        let mut reg = RoleRegistry::new();
        reg.define(Role::new("admin").with_permissions(["*"]));
        let effective = reg.effective_permissions(["admin"]);
        assert!(Authorizer::is_authorized(
            effective.iter(),
            &Permission::new("billing:refund")
        ));
    }

    #[test]
    fn authorize_returns_forbidden_domain_error() {
        let granted = perms(&["users:read"]);
        let err = Authorizer::authorize(&granted, &Permission::new("users:write")).unwrap_err();
        assert!(matches!(err, SecurityError::Forbidden));
        assert_eq!(err.category(), ErrorCategory::Forbidden);
        assert_eq!(err.code().as_str(), "security.forbidden");
    }
}
