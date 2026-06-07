//! [`Role`] (a named permission set) and the [`RoleRegistry`].

use std::collections::BTreeSet;

use super::Permission;

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
        Self { name: name.into(), permissions: BTreeSet::new() }
    }

    /// Builder: add `permissions` to this role.
    #[must_use]
    pub fn with_permissions<I, P>(mut self, permissions: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<Permission>,
    {
        self.permissions.extend(permissions.into_iter().map(Into::into));
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
