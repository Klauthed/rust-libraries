//! Evaluate a feature flag and resolve a tenant.
//!
//! Run with: `cargo run -p klauthed-platform --example platform_basics`

use klauthed_core::context::RequestContext;
use klauthed_platform::featureflag::{FeatureFlag, FeatureFlags, InMemoryFeatureFlags};
use klauthed_platform::tenancy::{InMemoryTenantResolver, Tenant, TenantResolver};

#[tokio::main]
async fn main() {
    // ── Feature flags ─────────────────────────────────────────────────────────
    let beta = FeatureFlag::new("beta-dashboard");
    let flags = InMemoryFeatureFlags::new().with_global(&beta, true);
    let ctx = RequestContext::new();
    println!("beta-dashboard enabled: {}", flags.is_enabled(&beta, &ctx));

    // ── Tenancy ───────────────────────────────────────────────────────────────
    let resolver =
        InMemoryTenantResolver::with_tenants([Tenant::new("acme").with_name("Acme, Inc.")]);
    match resolver.resolve("acme").await.unwrap() {
        Some(tenant) => println!("resolved '{}' → {}", tenant.slug(), tenant.name().unwrap_or("—")),
        None => println!("tenant not found"),
    }
}
