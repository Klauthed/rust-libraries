//! Public-API integration tests for the ABAC policy engine: condition
//! evaluation, deny-overrides, default-deny, and `authorize`.

use klauthed_security::SecurityError;
use klauthed_security::authz::{AttrValue, Attributes, Condition, Decision, Policy, PolicySet};

fn editor_request() -> Attributes {
    Attributes::new()
        .with("action", "articles:write")
        .with("subject.role", "editor")
        .with("subject.status", "active")
        .with("subject.groups", vec!["staff".to_owned(), "writers".to_owned()])
        .with("subject.level", 5_i64)
        .with("subject.mfa", true)
}

#[test]
fn condition_primitives_evaluate() {
    let attrs = editor_request();

    assert!(Condition::Always.evaluate(&attrs));
    assert!(Condition::eq("action", "articles:write").evaluate(&attrs));
    assert!(!Condition::eq("action", "articles:read").evaluate(&attrs));
    assert!(Condition::eq("subject.level", 5_i64).evaluate(&attrs));
    assert!(Condition::eq("subject.mfa", true).evaluate(&attrs));

    assert!(Condition::is_in("subject.role", ["editor", "admin"]).evaluate(&attrs));
    assert!(!Condition::is_in("subject.role", ["admin"]).evaluate(&attrs));

    assert!(Condition::contains("subject.groups", "writers").evaluate(&attrs));
    assert!(!Condition::contains("subject.groups", "admins").evaluate(&attrs));

    assert!(Condition::present("subject.role").evaluate(&attrs));
    assert!(!Condition::present("subject.absent").evaluate(&attrs));
}

#[test]
fn type_mismatches_do_not_match() {
    let attrs = Attributes::new().with("k", "text");
    // `In` only matches Str; `Contains` only matches List.
    assert!(!Condition::contains("k", "text").evaluate(&attrs));
    assert!(!Condition::is_in("missing", ["text"]).evaluate(&attrs));
    // Eq is type-aware: a string "5" is not the integer 5.
    let n = Attributes::new().with("k", 5_i64);
    assert!(!Condition::eq("k", "5").evaluate(&n));
    assert_eq!(n.get("k"), Some(&AttrValue::Int(5)));
}

#[test]
fn boolean_combinators() {
    let attrs = editor_request();

    let all = Condition::all([
        Condition::eq("action", "articles:write"),
        Condition::is_in("subject.role", ["editor", "admin"]),
    ]);
    assert!(all.evaluate(&attrs));

    let any = Condition::any([
        Condition::eq("subject.role", "admin"),
        Condition::eq("subject.role", "editor"),
    ]);
    assert!(any.evaluate(&attrs));

    assert!(Condition::negate(Condition::eq("subject.status", "suspended")).evaluate(&attrs));

    // Vacuous cases: empty All is true, empty Any is false.
    assert!(Condition::all([]).evaluate(&attrs));
    assert!(!Condition::any([]).evaluate(&attrs));
}

#[test]
fn permits_when_an_allow_matches() {
    let policies = PolicySet::new().with(Policy::allow(Condition::all([
        Condition::eq("action", "articles:write"),
        Condition::is_in("subject.role", ["editor", "admin"]),
    ])));
    assert_eq!(policies.evaluate(&editor_request()), Decision::Permit);
    assert!(policies.evaluate(&editor_request()).is_permit());
}

#[test]
fn deny_overrides_a_matching_allow() {
    let policies = PolicySet::new()
        .with(Policy::allow(Condition::eq("subject.role", "editor")))
        .with(Policy::deny(Condition::eq("subject.status", "suspended")));

    let suspended = editor_request().with("subject.status", "suspended");
    assert_eq!(policies.evaluate(&suspended), Decision::Deny);
}

#[test]
fn deny_overrides_regardless_of_order() {
    // Deny listed before the Allow still wins.
    let policies = PolicySet::new()
        .with(Policy::deny(Condition::eq("subject.status", "suspended")))
        .with(Policy::allow(Condition::eq("subject.role", "editor")));

    let suspended = editor_request().with("subject.status", "suspended");
    assert_eq!(policies.evaluate(&suspended), Decision::Deny);
}

#[test]
fn default_deny_when_nothing_matches() {
    // Empty set permits nothing.
    assert_eq!(PolicySet::new().evaluate(&editor_request()), Decision::Deny);

    // A non-matching allow leaves the default deny in place.
    let policies = PolicySet::new().with(Policy::allow(Condition::eq("subject.role", "admin")));
    assert_eq!(policies.evaluate(&editor_request()), Decision::Deny);
}

#[test]
fn authorize_maps_decision_to_result() {
    let allow_editors =
        PolicySet::new().with(Policy::allow(Condition::eq("subject.role", "editor")));
    assert!(allow_editors.authorize(&editor_request()).is_ok());

    let nobody = PolicySet::new();
    let err = nobody.authorize(&editor_request()).unwrap_err();
    assert!(matches!(err, SecurityError::Forbidden));
}
