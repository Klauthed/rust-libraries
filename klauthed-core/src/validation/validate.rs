//! The [`Validate`] trait.

use super::ValidationErrors;

/// Types that can validate their own invariants.
pub trait Validate {
    /// Check invariants, returning every problem found (not just the first).
    fn validate(&self) -> Result<(), ValidationErrors>;
}
