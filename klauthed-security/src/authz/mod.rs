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

pub mod authorizer;
pub mod permission;
pub mod role;

pub use authorizer::Authorizer;
pub use permission::Permission;
pub use role::{Role, RoleRegistry};
