//! Tests for the tenant model and resolver.

use klauthed_core::context::RequestContext;

use super::*;
use klauthed_error::{DomainError, ErrorCategory};

fn acme() -> Tenant {
    Tenant::new("acme").with_name("Acme, Inc.")
}

#[test]
fn tenant_builder_and_accessors() {
    let t = acme().with_metadata("plan", "pro");
    assert_eq!(t.slug(), "acme");
    assert_eq!(t.name(), Some("Acme, Inc."));
    assert_eq!(t.status(), TenantStatus::Active);
    assert!(t.is_active());
    assert_eq!(t.metadata_get("plan"), Some("pro"));
}

#[test]
fn ensure_active_rejects_suspended() {
    let t = acme().with_status(TenantStatus::Suspended);
    let err = t.ensure_active().unwrap_err();
    assert_eq!(err.category(), ErrorCategory::Forbidden);
    assert_eq!(err.code().as_str(), "platform.tenant_suspended");
}

#[test]
fn status_serde_is_snake_case() {
    let json = serde_json::to_string(&TenantStatus::Suspended).unwrap();
    assert_eq!(json, "\"suspended\"");
    let back: TenantStatus = serde_json::from_str("\"pending\"").unwrap();
    assert_eq!(back, TenantStatus::Pending);
}

#[test]
fn tenant_round_trips_through_json() {
    let t = acme().with_metadata("k", "v");
    let json = serde_json::to_string(&t).unwrap();
    let back: Tenant = serde_json::from_str(&json).unwrap();
    assert_eq!(t, back);
}

#[tokio::test]
async fn in_memory_resolver_by_id_and_slug() {
    let t = acme();
    let id = t.id();
    let resolver = InMemoryTenantResolver::with_tenants([t]);
    assert_eq!(resolver.len(), 1);

    let by_slug = resolver.resolve("acme").await.unwrap();
    assert_eq!(by_slug.as_ref().map(Tenant::id), Some(id));

    let by_id = resolver.resolve(&id.to_string()).await.unwrap();
    assert_eq!(by_id.map(|t| t.id()), Some(id));

    assert!(resolver.resolve("nope").await.unwrap().is_none());
}

#[tokio::test]
async fn require_maps_miss_to_not_found() {
    let resolver = InMemoryTenantResolver::new();
    let err = resolver.require("ghost").await.unwrap_err();
    assert_eq!(err.category(), ErrorCategory::NotFound);
    assert_eq!(err.code().as_str(), "platform.tenant_not_found");
}

#[tokio::test]
async fn from_context_uses_ctx_tenant() {
    let resolver = InMemoryTenantResolver::with_tenants([acme()]);

    let ctx = RequestContext::new().with_tenant("acme");
    let resolved = tenant_from_context(&resolver, &ctx).await.unwrap();
    assert_eq!(resolved.map(|t| t.slug().to_owned()), Some("acme".into()));

    let ctx = RequestContext::new();
    assert!(tenant_from_context(&resolver, &ctx).await.unwrap().is_none());
}
