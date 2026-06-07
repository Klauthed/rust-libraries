//! The [`FeatureFlag`] key newtype.

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
