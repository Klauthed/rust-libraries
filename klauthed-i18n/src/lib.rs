#![deny(unsafe_code)]
#![deny(missing_docs)]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

//! Internationalized messages for klauthed services.
//!
//! An [`I18n`] holds one [`Catalog`] per [`Locale`] and resolves a dotted
//! message key (`validation.required`) to a localized, interpolated string.
//! Lookup falls back **exact locale → primary language → default locale → the
//! key itself**, so a missing translation degrades gracefully.
//!
//! The framework ships default catalogs (en, de, es, fr, it, tr) embedded in the
//! binary; services can override individual messages or add locales at runtime.
//!
//! ```
//! use klauthed_i18n::{Args, I18n, Locale};
//!
//! let i18n = I18n::with_embedded_defaults();
//!
//! let msg = i18n.translate_with(
//!     &Locale::new("tr"),
//!     "validation.required",
//!     &Args::new().set("field", "e-posta"),
//! );
//! assert_eq!(msg, "e-posta alanı zorunludur.");
//!
//! // en-US falls back to the en catalog:
//! let en = i18n.translate_with(
//!     &Locale::new("en-US"),
//!     "validation.required",
//!     &Args::new().set("field", "email"),
//! );
//! assert_eq!(en, "The field email is required.");
//! ```

mod catalog;
mod error;
mod format;
mod locale;

pub use catalog::Catalog;
pub use error::I18nError;
pub use format::Args;
pub use locale::Locale;

use std::collections::HashMap;
use std::path::Path;

/// The framework's built-in catalogs (locale tag, embedded TOML source).
const EMBEDDED: &[(&str, &str)] = &[
    ("en", include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/locales/en.toml"))),
    ("de", include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/locales/de.toml"))),
    ("es", include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/locales/es.toml"))),
    ("fr", include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/locales/fr.toml"))),
    ("it", include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/locales/it.toml"))),
    ("tr", include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/locales/tr.toml"))),
];

/// A resolved set of message catalogs with a default fallback locale.
pub struct I18n {
    catalogs: HashMap<Locale, Catalog>,
    default: Locale,
}

impl I18n {
    /// Start building an `I18n`.
    pub fn builder() -> I18nBuilder {
        I18nBuilder::new()
    }

    /// Build with the embedded framework catalogs and default locale `en`.
    #[allow(
        clippy::expect_used,
        reason = "the embedded catalogs are compile-time constants validated by a test"
    )]
    pub fn with_embedded_defaults() -> Self {
        I18nBuilder::new().embedded_defaults().expect("embedded catalogs are valid TOML").build()
    }

    /// The fallback locale used when a translation is missing.
    pub fn default_locale(&self) -> &Locale {
        &self.default
    }

    /// Whether a catalog exists for `locale` (exact match).
    pub fn has_locale(&self, locale: &Locale) -> bool {
        self.catalogs.contains_key(locale)
    }

    /// The loaded locales.
    pub fn locales(&self) -> impl Iterator<Item = &Locale> {
        self.catalogs.keys()
    }

    /// Translate `key` for `locale` with no arguments.
    pub fn translate(&self, locale: &Locale, key: &str) -> String {
        self.translate_with(locale, key, &Args::new())
    }

    /// Translate `key` for `locale`, interpolating `args`. Falls back to the
    /// primary subtag, then the default locale, then the key itself.
    pub fn translate_with(&self, locale: &Locale, key: &str, args: &Args) -> String {
        match self.resolve(locale, key) {
            Some(template) => format::interpolate(template, args),
            None => key.to_owned(),
        }
    }

    /// The raw (un-interpolated) template for `key`/`locale`, applying the same
    /// fallback chain, if any catalog has it.
    pub fn message(&self, locale: &Locale, key: &str) -> Option<&str> {
        self.resolve(locale, key)
    }

    /// Resolve a template through the fallback chain.
    fn resolve(&self, locale: &Locale, key: &str) -> Option<&str> {
        if let Some(catalog) = self.catalogs.get(locale)
            && let Some(message) = catalog.get(key)
        {
            return Some(message);
        }
        let primary = locale.primary();
        if primary != *locale
            && let Some(catalog) = self.catalogs.get(&primary)
            && let Some(message) = catalog.get(key)
        {
            return Some(message);
        }
        if let Some(catalog) = self.catalogs.get(&self.default)
            && let Some(message) = catalog.get(key)
        {
            return Some(message);
        }
        None
    }
}

/// Builder for [`I18n`].
pub struct I18nBuilder {
    catalogs: HashMap<Locale, Catalog>,
    default: Locale,
}

impl I18nBuilder {
    /// A builder with no catalogs and default locale `en`.
    pub fn new() -> Self {
        Self { catalogs: HashMap::new(), default: Locale::new("en") }
    }

    /// Set the fallback locale.
    pub fn default_locale(mut self, locale: impl Into<Locale>) -> Self {
        self.default = locale.into();
        self
    }

    /// Add the embedded framework catalogs (merging over anything already added).
    pub fn embedded_defaults(mut self) -> Result<Self, I18nError> {
        for (tag, toml_str) in EMBEDDED {
            self = self.add_catalog(*tag, toml_str)?;
        }
        Ok(self)
    }

    /// Add (or override into) a catalog from TOML text. Entries merge over any
    /// existing catalog for the same locale, so individual keys can be overridden.
    pub fn add_catalog(
        mut self,
        locale: impl Into<Locale>,
        toml_str: &str,
    ) -> Result<Self, I18nError> {
        let locale = locale.into();
        let catalog = Catalog::from_toml_str(locale.as_str(), toml_str)?;
        self.catalogs.entry(locale).or_default().merge(catalog);
        Ok(self)
    }

    /// Load every `*.toml` in `dir` as a catalog whose locale is the file stem
    /// (e.g. `tr.toml` → `tr`), merging over existing entries.
    pub fn load_dir(mut self, dir: impl AsRef<Path>) -> Result<Self, I18nError> {
        let dir = dir.as_ref();
        let entries = std::fs::read_dir(dir).map_err(|e| I18nError::Io {
            path: dir.display().to_string(),
            message: e.to_string(),
        })?;
        for entry in entries {
            let path = entry
                .map_err(|e| I18nError::Io {
                    path: dir.display().to_string(),
                    message: e.to_string(),
                })?
                .path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let text = std::fs::read_to_string(&path).map_err(|e| I18nError::Io {
                path: path.display().to_string(),
                message: e.to_string(),
            })?;
            self = self.add_catalog(stem, &text)?;
        }
        Ok(self)
    }

    /// Finalize the [`I18n`].
    pub fn build(self) -> I18n {
        I18n { catalogs: self.catalogs, default: self.default }
    }
}

impl Default for I18nBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_with_interpolation_in_multiple_locales() {
        let i18n = I18n::with_embedded_defaults();

        let en = i18n.translate_with(
            &Locale::new("en"),
            "validation.required",
            &Args::new().set("field", "email"),
        );
        assert_eq!(en, "The field email is required.");

        let tr = i18n.translate_with(
            &Locale::new("tr"),
            "validation.required",
            &Args::new().set("field", "e-posta"),
        );
        assert_eq!(tr, "e-posta alanı zorunludur.");
    }

    #[test]
    fn region_falls_back_to_primary_language() {
        let i18n = I18n::with_embedded_defaults();
        let msg = i18n.translate(&Locale::new("de-AT"), "not_found.generic");
        assert_eq!(msg, i18n.translate(&Locale::new("de"), "not_found.generic"));
        assert_ne!(msg, "not_found.generic");
    }

    #[test]
    fn unknown_locale_falls_back_to_default() {
        let i18n = I18n::with_embedded_defaults();
        // `jp` has no catalog → default `en`.
        let msg = i18n.translate(&Locale::new("jp"), "conflict.generic");
        assert_eq!(msg, "The operation conflicts with the current state of the resource.");
    }

    #[test]
    fn missing_key_returns_the_key() {
        let i18n = I18n::with_embedded_defaults();
        assert_eq!(i18n.translate(&Locale::new("en"), "does.not.exist"), "does.not.exist");
    }

    #[test]
    fn runtime_override_wins_over_embedded() {
        let i18n = I18n::builder()
            .embedded_defaults()
            .unwrap()
            .add_catalog("en", "[internal]\nerror = \"Custom error message.\"")
            .unwrap()
            .build();
        assert_eq!(i18n.translate(&Locale::new("en"), "internal.error"), "Custom error message.");
        // A non-overridden key is still present.
        assert_eq!(i18n.translate(&Locale::new("en"), "tenant.not_found"), "Tenant not found.");
    }
}
