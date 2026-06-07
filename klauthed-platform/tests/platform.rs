//! Public-API integration tests for the platform primitives.

use klauthed_core::context::RequestContext;
use klauthed_platform::featureflag::{FeatureFlag, FeatureFlags, InMemoryFeatureFlags};
use klauthed_platform::tenancy::{InMemoryTenantResolver, Tenant, TenantResolver};

#[test]
fn feature_flags_resolve_globally() {
    let beta = FeatureFlag::new("beta-dashboard");
    let flags = InMemoryFeatureFlags::new().with_global(&beta, true);
    let ctx = RequestContext::new();

    assert!(flags.is_enabled(&beta, &ctx));
    assert!(!flags.is_enabled(&FeatureFlag::new("unknown-flag"), &ctx));
}

#[tokio::test]
async fn tenants_resolve_by_slug() {
    let resolver = InMemoryTenantResolver::with_tenants([Tenant::new("acme")]);

    assert!(resolver.resolve("acme").await.unwrap().is_some());
    assert!(resolver.resolve("nope").await.unwrap().is_none());
    // `require` maps a miss to an error.
    assert!(resolver.require("nope").await.is_err());
}
