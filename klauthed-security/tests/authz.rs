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

#[test]
fn roles_inherit_parent_permissions_transitively() {
    let mut reg = RoleRegistry::new();
    reg.define(Role::new("viewer").with_permissions([Permission::new("articles:read")]));
    reg.define(
        Role::new("editor")
            .with_permissions([Permission::new("articles:write")])
            .inherits(["viewer"]),
    );
    // admin -> editor -> viewer (two levels deep).
    reg.define(
        Role::new("admin").with_permissions([Permission::new("users:delete")]).inherits(["editor"]),
    );

    let admin = reg.effective_permissions(["admin"]);
    // Direct + transitively inherited permissions are all present.
    assert!(Authorizer::is_authorized(&admin, &Permission::new("users:delete")));
    assert!(Authorizer::is_authorized(&admin, &Permission::new("articles:write")));
    assert!(Authorizer::is_authorized(&admin, &Permission::new("articles:read")));

    // viewer alone has only its own.
    let viewer = reg.effective_permissions(["viewer"]);
    assert!(Authorizer::is_authorized(&viewer, &Permission::new("articles:read")));
    assert!(!Authorizer::is_authorized(&viewer, &Permission::new("articles:write")));
}

#[test]
fn inheritance_cycles_are_handled() {
    let mut reg = RoleRegistry::new();
    // a -> b -> a (a cycle).
    reg.define(Role::new("a").with_permissions([Permission::new("a:act")]).inherits(["b"]));
    reg.define(Role::new("b").with_permissions([Permission::new("b:act")]).inherits(["a"]));

    // Resolution terminates and unions both, despite the cycle.
    let perms = reg.effective_permissions(["a"]);
    assert!(Authorizer::is_authorized(&perms, &Permission::new("a:act")));
    assert!(Authorizer::is_authorized(&perms, &Permission::new("b:act")));
    assert_eq!(perms.len(), 2);
}

#[test]
fn unknown_parent_is_ignored() {
    let mut reg = RoleRegistry::new();
    reg.define(Role::new("svc").with_permissions([Permission::new("svc:run")]).inherits(["ghost"]));
    let perms = reg.effective_permissions(["svc"]);
    assert_eq!(perms.len(), 1);
    assert!(Authorizer::is_authorized(&perms, &Permission::new("svc:run")));
}
