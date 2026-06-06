//! A message catalog: dotted keys to message templates for one locale.

use std::collections::BTreeMap;

use crate::error::I18nError;

/// The messages for a single locale, addressed by dotted key
/// (`section.key`, matching the `[section]` tables in the TOML files).
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    messages: BTreeMap<String, String>,
}

impl Catalog {
    /// An empty catalog.
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse a catalog from TOML text. `[section]` tables become `section.key`
    /// entries; nested tables nest further. Non-string leaves are ignored.
    pub fn from_toml_str(locale: &str, toml_str: &str) -> Result<Self, I18nError> {
        let table: toml::Table = toml_str.parse().map_err(|e: toml::de::Error| {
            I18nError::Parse { locale: locale.to_owned(), message: e.to_string() }
        })?;
        let mut messages = BTreeMap::new();
        flatten(&table, "", &mut messages);
        Ok(Self { messages })
    }

    /// Look up a message template by dotted key.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.messages.get(key).map(String::as_str)
    }

    /// Number of messages.
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// Whether the catalog has no messages.
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// Merge `other` into this catalog, with `other`'s entries overriding
    /// existing keys (used to layer overrides over the embedded defaults).
    pub fn merge(&mut self, other: Catalog) {
        self.messages.extend(other.messages);
    }
}

/// Recursively flatten a TOML table into dotted string keys.
fn flatten(table: &toml::Table, prefix: &str, out: &mut BTreeMap<String, String>) {
    for (key, value) in table {
        let full = if prefix.is_empty() { key.clone() } else { format!("{prefix}.{key}") };
        match value {
            toml::Value::String(text) => {
                out.insert(full, text.clone());
            }
            toml::Value::Table(child) => flatten(child, &full, out),
            // Messages are strings; ignore other leaf types.
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flattens_sections_into_dotted_keys() {
        let catalog = Catalog::from_toml_str(
            "en",
            r#"
            [validation]
            required = "The field {field} is required."
            [user]
            not_found = "User {id} not found."
            "#,
        )
        .unwrap();

        assert_eq!(catalog.len(), 2);
        assert_eq!(catalog.get("validation.required"), Some("The field {field} is required."));
        assert_eq!(catalog.get("user.not_found"), Some("User {id} not found."));
        assert_eq!(catalog.get("missing.key"), None);
    }

    #[test]
    fn merge_overrides_existing_keys() {
        let mut base = Catalog::from_toml_str(
            "en",
            r#"[a]
        b = "base"
        c = "keep""#,
        )
        .unwrap();
        let over = Catalog::from_toml_str(
            "en",
            r#"[a]
        b = "override""#,
        )
        .unwrap();
        base.merge(over);
        assert_eq!(base.get("a.b"), Some("override"));
        assert_eq!(base.get("a.c"), Some("keep"));
    }
}
