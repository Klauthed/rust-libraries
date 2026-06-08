//! `klauthed-security::authz`: RBAC with role inheritance, resource-instance
//! scoping, and the ABAC policy engine.

use klauthed_security::authz::{Attributes, Condition, Decision, Policy, PolicySet};
use klauthed_security::{Authorizer, Permission, Role, RoleRegistry};

pub fn run() {
    // RBAC + inheritance: editor inherits viewer's permissions.
    let mut reg = RoleRegistry::new();
    reg.define(Role::new("viewer").with_permissions([Permission::new("articles:read")]));
    reg.define(
        Role::new("editor")
            .with_permissions([Permission::new("articles:write")])
            .inherits(["viewer"]),
    );
    let editor = reg.effective_permissions(["editor"]);
    assert!(Authorizer::is_authorized(&editor, &Permission::new("articles:read"))); // inherited
    assert!(Authorizer::is_authorized(&editor, &Permission::new("articles:write")));
    println!("  rbac: editor inherits viewer -> {} effective permissions", editor.len());

    // Resource-instance scoping via the `:own` convention.
    let own = [Permission::new("articles:edit:own")];
    let edit = Permission::new("articles:edit");
    assert!(Authorizer::is_authorized_for_resource(&own, &edit, "alice", "alice"));
    assert!(!Authorizer::is_authorized_for_resource(&own, &edit, "alice", "bob"));
    println!("  resource-scope: `:own` grant edits own records, not others'");

    // ABAC: condition-based policies, deny-overrides, default-deny.
    let policies = PolicySet::new()
        .with(Policy::allow(Condition::all([
            Condition::eq("action", "articles:write"),
            Condition::is_in("subject.role", ["editor", "admin"]),
        ])))
        .with(Policy::deny(Condition::eq("subject.status", "suspended")));

    let active = Attributes::new()
        .with("action", "articles:write")
        .with("subject.role", "editor")
        .with("subject.status", "active");
    assert_eq!(policies.evaluate(&active), Decision::Permit);

    let suspended = active.clone().with("subject.status", "suspended");
    assert_eq!(policies.evaluate(&suspended), Decision::Deny); // deny wins
    println!("  abac: active editor permitted; suspended denied (deny-overrides)");
}
