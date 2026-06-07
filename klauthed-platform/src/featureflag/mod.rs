//! Feature flags with optional per-tenant overrides.
//!
//! A [`FeatureFlag`] is a stable string key. A [`FeatureFlags`] provider answers
//! [`is_enabled`](FeatureFlags::is_enabled) and [`variant`](FeatureFlags::variant)
//! for a flag in a given [`RequestContext`](klauthed_core::context::RequestContext). [`InMemoryFeatureFlags`] is a simple,
//! deterministic provider: a global default per flag, optionally overridden per
//! tenant (keyed by the context's [`tenant`](klauthed_core::context::RequestContext::tenant)).
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

pub mod flag;
pub mod memory;
pub mod provider;

pub use flag::FeatureFlag;
pub use memory::InMemoryFeatureFlags;
pub use provider::FeatureFlags;
