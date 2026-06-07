//! Public-API integration tests for permissions, roles, and authorization.

use klauthed_error::{DomainError, ErrorCategory};
use klauthed_security::{Authorizer, Permission, Role, RoleRegistry, SecurityError};

fn perms(list: &[&str]) -> Vec<Permission> {
    list.iter().map(|s| Permission::new(*s)).collect()
}

#[test]
fn exact_match_allows() {
    let granted = perms(&["users:read"]);
    assert!(Authorizer::is_authorized(&granted, &Permission::new("users:read")));
}

#[test]
fn missing_permission_denies() {
    let granted = perms(&["users:read"]);
    assert!(!Authorizer::is_authorized(&granted, &Permission::new("users:write")));
}

#[test]
fn resource_wildcard_matches_actions() {
    let granted = perms(&["users:*"]);
    assert!(Authorizer::is_authorized(&granted, &Permission::new("users:read")));
    assert!(Authorizer::is_authorized(&granted, &Permission::new("users:delete")));
    // Different resource is not covered.
    assert!(!Authorizer::is_authorized(&granted, &Permission::new("orders:read")));
}

#[test]
fn global_wildcard_matches_everything() {
    let granted = perms(&["*"]);
    assert!(Authorizer::is_authorized(&granted, &Permission::new("anything:goes")));
    assert!(Authorizer::is_authorized(&granted, &Permission::new("a:b:c")));
}

#[test]
fn mid_segment_wildcard_matches_one_segment() {
    let granted = perms(&["users:*:read"]);
    assert!(Authorizer::is_authorized(&granted, &Permission::new("users:42:read")));
    // Length mismatch is not a grant.
    assert!(!Authorizer::is_authorized(&granted, &Permission::new("users:42")));
}

#[test]
fn resource_wildcard_does_not_grant_shorter_requirement() {
    // "users:*" should not grant the bare "users".
    let granted = perms(&["users:*"]);
    assert!(!Authorizer::is_authorized(&granted, &Permission::new("users")));
}

#[test]
fn registry_resolves_effective_permissions_from_roles() {
    let mut reg = RoleRegistry::new();
    reg.define(Role::new("reader").with_permissions([Permission::new("articles:read")]));
    reg.define(Role::new("writer").with_permissions(["articles:write"]));

    let effective = reg.effective_permissions(["reader", "writer", "unknown"]);
    let collected: Vec<&Permission> = effective.iter().collect();
    assert!(Authorizer::is_authorized(
        collected.iter().copied(),
        &Permission::new("articles:read")
    ));
    assert!(Authorizer::is_authorized(effective.iter(), &Permission::new("articles:write")));
    assert!(!Authorizer::is_authorized(effective.iter(), &Permission::new("articles:delete")));
}

#[test]
fn admin_role_with_global_wildcard() {
    let mut reg = RoleRegistry::new();
    reg.define(Role::new("admin").with_permissions(["*"]));
    let effective = reg.effective_permissions(["admin"]);
    assert!(Authorizer::is_authorized(effective.iter(), &Permission::new("billing:refund")));
}

#[test]
fn authorize_returns_forbidden_domain_error() {
    let granted = perms(&["users:read"]);
    let err = Authorizer::authorize(&granted, &Permission::new("users:write")).unwrap_err();
    assert!(matches!(err, SecurityError::Forbidden));
    assert_eq!(err.category(), ErrorCategory::Forbidden);
    assert_eq!(err.code().as_str(), "security.forbidden");
}
