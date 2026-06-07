//! The [`FeatureFlags`] evaluation trait.

use klauthed_core::context::RequestContext;

use super::FeatureFlag;

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
