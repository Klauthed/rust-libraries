//! Public-API integration tests for feature flags: key string/serde forms and
//! the global/per-tenant resolution order for booleans and variants.

use klauthed_core::context::RequestContext;
use klauthed_platform::featureflag::{FeatureFlag, FeatureFlags, InMemoryFeatureFlags};

#[test]
fn flag_key_string_forms() {
    let f = FeatureFlag::new("a.b");
    assert_eq!(f.as_str(), "a.b");
    assert_eq!(f.to_string(), "a.b");
    assert_eq!(FeatureFlag::from("x"), FeatureFlag::new("x"));
}

#[test]
fn flag_serde_is_transparent_string() {
    let f = FeatureFlag::new("beta");
    assert_eq!(serde_json::to_string(&f).unwrap(), "\"beta\"");
    let back: FeatureFlag = serde_json::from_str("\"beta\"").unwrap();
    assert_eq!(back, f);
}

#[test]
fn unknown_flag_is_off() {
    let flags = InMemoryFeatureFlags::new();
    assert!(!flags.is_enabled(&FeatureFlag::new("nope"), &RequestContext::new()));
}

#[test]
fn global_then_tenant_override() {
    let beta = FeatureFlag::new("beta");
    let flags = InMemoryFeatureFlags::new()
        .with_global(&beta, false)
        .with_tenant_override("acme", &beta, true)
        .with_tenant_override("globex", &beta, false);

    assert!(!flags.is_enabled(&beta, &RequestContext::new()));
    assert!(flags.is_enabled(&beta, &RequestContext::new().with_tenant("acme")));
    assert!(!flags.is_enabled(&beta, &RequestContext::new().with_tenant("globex")));
    // A tenant without an override falls back to the global default.
    assert!(!flags.is_enabled(&beta, &RequestContext::new().with_tenant("other")));
}

#[test]
fn variants_resolve_tenant_then_global() {
    let theme = FeatureFlag::new("theme");
    let flags = InMemoryFeatureFlags::new()
        .with_global_variant(&theme, "light")
        .with_tenant_variant("acme", &theme, "dark");

    assert_eq!(flags.variant(&theme, &RequestContext::new()), Some("light".into()));
    assert_eq!(
        flags.variant(&theme, &RequestContext::new().with_tenant("acme")),
        Some("dark".into())
    );
    assert_eq!(flags.variant(&FeatureFlag::new("missing"), &RequestContext::new()), None);
}
