//! The [`InMemoryFeatureFlags`] provider and its per-flag rule.

use std::collections::BTreeMap;

use klauthed_core::context::RequestContext;

use super::{FeatureFlag, FeatureFlags};

/// Per-flag rule: a global default plus optional per-tenant boolean overrides
/// and per-tenant variant strings.
#[derive(Debug, Clone, Default)]
struct FlagRule {
    global: bool,
    tenant_enabled: BTreeMap<String, bool>,
    global_variant: Option<String>,
    tenant_variant: BTreeMap<String, String>,
}

/// An in-memory, statically-configured [`FeatureFlags`] provider.
///
/// Resolution order for both `is_enabled` and `variant`: the per-tenant override
/// (matched on the context's [`tenant`](RequestContext::tenant)) wins; otherwise
/// the global default applies; an unknown flag is off / `None`.
#[derive(Debug, Clone, Default)]
pub struct InMemoryFeatureFlags {
    rules: BTreeMap<FeatureFlag, FlagRule>,
}

impl InMemoryFeatureFlags {
    /// An empty provider (every flag off).
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the global default for `flag` (builder form).
    pub fn with_global(mut self, flag: &FeatureFlag, enabled: bool) -> Self {
        self.rules.entry(flag.clone()).or_default().global = enabled;
        self
    }

    /// Override `flag` for a specific tenant (builder form).
    pub fn with_tenant_override(
        mut self,
        tenant: impl Into<String>,
        flag: &FeatureFlag,
        enabled: bool,
    ) -> Self {
        self.rules.entry(flag.clone()).or_default().tenant_enabled.insert(tenant.into(), enabled);
        self
    }

    /// Set the global multivariate value for `flag` (builder form).
    pub fn with_global_variant(mut self, flag: &FeatureFlag, variant: impl Into<String>) -> Self {
        self.rules.entry(flag.clone()).or_default().global_variant = Some(variant.into());
        self
    }

    /// Override `flag`'s multivariate value for a tenant (builder form).
    pub fn with_tenant_variant(
        mut self,
        tenant: impl Into<String>,
        flag: &FeatureFlag,
        variant: impl Into<String>,
    ) -> Self {
        self.rules
            .entry(flag.clone())
            .or_default()
            .tenant_variant
            .insert(tenant.into(), variant.into());
        self
    }
}

impl FeatureFlags for InMemoryFeatureFlags {
    fn is_enabled(&self, flag: &FeatureFlag, ctx: &RequestContext) -> bool {
        let Some(rule) = self.rules.get(flag) else {
            return false;
        };
        if let Some(tenant) = ctx.tenant()
            && let Some(&overridden) = rule.tenant_enabled.get(tenant)
        {
            return overridden;
        }
        rule.global
    }

    fn variant(&self, flag: &FeatureFlag, ctx: &RequestContext) -> Option<String> {
        let rule = self.rules.get(flag)?;
        if let Some(tenant) = ctx.tenant()
            && let Some(value) = rule.tenant_variant.get(tenant)
        {
            return Some(value.clone());
        }
        rule.global_variant.clone()
    }
}
