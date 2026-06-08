//! [`Role`] (a named permission set) and the [`RoleRegistry`].

use std::collections::BTreeSet;

use super::Permission;

/// A named set of [`Permission`]s, optionally inheriting from parent roles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Role {
    /// The role's unique name (e.g. `"admin"`).
    pub name: String,
    /// The permissions granted directly by holding this role.
    pub permissions: BTreeSet<Permission>,
    /// Names of roles this role inherits permissions from. Resolved transitively
    /// (and cycle-safely) by [`RoleRegistry::effective_permissions`].
    pub parents: BTreeSet<String>,
}

impl Role {
    /// A role named `name` with no permissions or parents yet.
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), permissions: BTreeSet::new(), parents: BTreeSet::new() }
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

    /// Builder: declare that this role inherits the named parent roles'
    /// permissions (resolved transitively at lookup time).
    ///
    /// Parents are referenced by name and resolved lazily, so a role may be
    /// defined before its parents and unknown parents are simply skipped.
    #[must_use]
    pub fn inherits<I, S>(mut self, parents: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.parents.extend(parents.into_iter().map(Into::into));
        self
    }

    /// Add a single permission in place.
    pub fn grant(&mut self, permission: impl Into<Permission>) {
        self.permissions.insert(permission.into());
    }

    /// Add a single parent role in place.
    pub fn inherit(&mut self, parent: impl Into<String>) {
        self.parents.insert(parent.into());
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
    /// holds the given role names, **including inherited roles**.
    ///
    /// Each role's [`parents`](Role::parents) are followed transitively; the
    /// traversal is cycle-safe (a role is expanded at most once, so inheritance
    /// cycles and diamonds are handled) and unknown role names are ignored.
    pub fn effective_permissions<I, S>(&self, role_names: I) -> BTreeSet<Permission>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut perms = BTreeSet::new();
        let mut visited: BTreeSet<String> = BTreeSet::new();
        let mut stack: Vec<String> =
            role_names.into_iter().map(|s| s.as_ref().to_owned()).collect();

        while let Some(name) = stack.pop() {
            // `insert` returns false if already present — visiting each role once
            // makes the walk terminate even with inheritance cycles.
            if !visited.insert(name.clone()) {
                continue;
            }
            if let Some(role) = self.roles.get(&name) {
                perms.extend(role.permissions.iter().cloned());
                stack.extend(role.parents.iter().cloned());
            }
        }
        perms
    }
}
