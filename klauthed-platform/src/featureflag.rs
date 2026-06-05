//! Feature flags with optional per-tenant overrides.
//!
//! A [`FeatureFlag`] is a stable string key. A [`FeatureFlags`] provider answers
//! [`is_enabled`](FeatureFlags::is_enabled) and [`variant`](FeatureFlags::variant)
//! for a flag in a given [`RequestContext`]. [`InMemoryFeatureFlags`] is a simple,
//! deterministic provider: a global default per flag, optionally overridden per
//! tenant (keyed by the context's [`tenant`](RequestContext::tenant)).
//!
//! ```
//! use klauthed_core::context::RequestContext;
//! use klauthed_platform::featureflag::{FeatureFlag, FeatureFlags, InMemoryFeatureFlags};
//!
//! let beta = FeatureFlag::new("beta_ui");
//! let flags = InMemoryFeatureFlags::new()
//!     .with_global(&beta, false)
//!     .with_tenant_override("acme", &beta, true);
//!
//! let anon = RequestContext::new();
//! assert!(!flags.is_enabled(&beta, &anon));
//!
//! let acme = RequestContext::new().with_tenant("acme");
//! assert!(flags.is_enabled(&beta, &acme));
//! ```

use std::collections::BTreeMap;

use klauthed_core::context::RequestContext;
use serde::{Deserialize, Serialize};

/// A stable feature-flag key (a newtype over a `String`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FeatureFlag(String);

impl FeatureFlag {
    /// Construct a flag key.
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    /// The underlying key string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for FeatureFlag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for FeatureFlag {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for FeatureFlag {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// Evaluates feature flags for a [`RequestContext`].
///
/// Implementors are `Send + Sync` so a provider can be shared as
/// `Arc<dyn FeatureFlags>`. Evaluation is expected to be cheap and deterministic.
pub trait FeatureFlags: Send + Sync {
    /// Whether `flag` is on for `ctx`. Unknown flags are off.
    fn is_enabled(&self, flag: &FeatureFlag, ctx: &RequestContext) -> bool;

    /// The multivariate value for `flag` in `ctx`, or `None` if unset.
    ///
    /// Multivariate flags are orthogonal to the boolean on/off switch; a default
    /// provider that only tracks booleans returns `None`.
    fn variant(&self, _flag: &FeatureFlag, _ctx: &RequestContext) -> Option<String> {
        None
    }
}

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
        self.rules
            .entry(flag.clone())
            .or_default()
            .tenant_enabled
            .insert(tenant.into(), enabled);
        self
    }

    /// Set the global multivariate value for `flag` (builder form).
    pub fn with_global_variant(
        mut self,
        flag: &FeatureFlag,
        variant: impl Into<String>,
    ) -> Self {
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

#[cfg(test)]
mod tests {
    use super::*;

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

        assert_eq!(
            flags.variant(&theme, &RequestContext::new()),
            Some("light".into())
        );
        assert_eq!(
            flags.variant(&theme, &RequestContext::new().with_tenant("acme")),
            Some("dark".into())
        );
        assert_eq!(flags.variant(&FeatureFlag::new("missing"), &RequestContext::new()), None);
    }
}
