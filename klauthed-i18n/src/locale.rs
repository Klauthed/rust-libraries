//! Locale identifiers.

use std::fmt;
use std::str::FromStr;

/// A normalized locale tag (lowercased), e.g. `en`, `tr`, `en-us`.
///
/// Region-specific tags fall back to their primary language subtag during
/// lookup, so `en-US` resolves against an `en` catalog when no `en-us` exists.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Locale(String);

impl Locale {
    /// Build a normalized locale from any tag (trimmed and lowercased).
    pub fn new(tag: impl AsRef<str>) -> Self {
        Self(normalize(tag.as_ref()))
    }

    /// The tag as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The primary language subtag (`en-us` → `en`). Returns a clone when the
    /// locale is already a bare language.
    #[must_use]
    pub fn primary(&self) -> Locale {
        match self.0.split_once(['-', '_']) {
            Some((lang, _)) => Locale(lang.to_owned()),
            None => self.clone(),
        }
    }

    /// Whether this locale has a region/script subtag.
    pub fn has_region(&self) -> bool {
        self.0.contains(['-', '_'])
    }
}

fn normalize(tag: &str) -> String {
    tag.trim().to_ascii_lowercase().replace('_', "-")
}

impl fmt::Display for Locale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for Locale {
    fn from(tag: &str) -> Self {
        Locale::new(tag)
    }
}

impl From<String> for Locale {
    fn from(tag: String) -> Self {
        Locale::new(tag)
    }
}

impl FromStr for Locale {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Locale::new(s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_and_extracts_primary() {
        let loc = Locale::new("EN_us");
        assert_eq!(loc.as_str(), "en-us");
        assert!(loc.has_region());
        assert_eq!(loc.primary(), Locale::new("en"));
        assert_eq!(Locale::new("tr").primary(), Locale::new("tr"));
    }
}
