use serde_json::Value;

use super::{ConfigProvider, ProviderKind};
use crate::config::map::ConfigMap;
use crate::error::ConfigError;

/// Default prefix for configuration environment variables.
const DEFAULT_PREFIX: &str = "APP";

/// Separator within a variable name that maps to nesting in the config tree.
/// `APP_DATABASE__POOL__MAX` → `database.pool.max`.
const NESTING_SEPARATOR: &str = "__";

/// Reads configuration from process environment variables.
///
/// Only variables matching `{PREFIX}_` are considered (default prefix `APP`), so
/// unrelated environment (`PATH`, `HOME`, …) never leaks into the config tree.
/// The prefix and a leading `_` are stripped, the name is lowercased, and the
/// nesting separator `__` is mapped to `.` then expanded into nested objects:
///
/// ```text
/// APP_DATABASE__POOL__MAX=20   →  { "database": { "pool": { "max": 20 } } }
/// ```
///
/// Scalar string values are coerced to booleans/numbers/null where they parse
/// cleanly, so typed fields (`u32`, `bool`, …) deserialize without the caller
/// having to quote-and-parse.
pub struct EnvProvider {
    prefix: String,
}

impl EnvProvider {
    /// Provider using the default `APP` prefix.
    pub fn new() -> Self {
        Self {
            prefix: DEFAULT_PREFIX.to_owned(),
        }
    }

    /// Provider using a custom prefix (without the trailing underscore).
    pub fn with_prefix(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
        }
    }

    /// Collect matching variables from an arbitrary iterator. Kept separate from
    /// [`Self::load`] so it can be unit-tested without touching the real
    /// process environment.
    fn collect<I>(&self, vars: I) -> ConfigMap
    where
        I: IntoIterator<Item = (String, String)>,
    {
        let match_prefix = format!("{}_", self.prefix.to_uppercase());

        let flat: ConfigMap = vars
            .into_iter()
            .filter_map(|(key, value)| {
                let rest = key.to_uppercase().strip_prefix(&match_prefix)?.to_owned();
                if rest.is_empty() {
                    return None;
                }
                let config_key = rest.to_lowercase().replace(NESTING_SEPARATOR, ".");
                Some((config_key, coerce_scalar(value)))
            })
            .collect();

        flat.expand_dotted()
    }
}

impl Default for EnvProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ConfigProvider for EnvProvider {
    fn name(&self) -> String {
        format!("env:{}_*", self.prefix.to_uppercase())
    }

    fn kind(&self) -> ProviderKind {
        ProviderKind::Env
    }

    async fn load(&self) -> Result<ConfigMap, ConfigError> {
        Ok(self.collect(std::env::vars()))
    }
}

/// Best-effort coercion of a raw env string into a typed JSON scalar.
///
/// Tries, in order: `bool`, `null`, signed int, unsigned int, float. Anything
/// else stays a string. This keeps `APP_DEBUG=true` and `APP_DB__POOL__MAX=20`
/// usable as `bool`/`u32` without surprising callers — note that purely numeric
/// strings become numbers, so values that must stay strings (zip codes, ids)
/// should be set via file/Vault providers, not env.
fn coerce_scalar(raw: String) -> Value {
    match raw.as_str() {
        "true" => return Value::Bool(true),
        "false" => return Value::Bool(false),
        "null" => return Value::Null,
        _ => {}
    }
    if let Ok(i) = raw.parse::<i64>() {
        return Value::from(i);
    }
    if let Ok(u) = raw.parse::<u64>() {
        return Value::from(u);
    }
    if let Ok(f) = raw.parse::<f64>()
        && f.is_finite()
    {
        return Value::from(f);
    }
    Value::String(raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn vars(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn filters_by_prefix_and_ignores_unrelated_env() {
        let provider = EnvProvider::new();
        let out = provider.collect(vars(&[
            ("PATH", "/usr/bin"),
            ("HOME", "/home/x"),
            ("APP_NAME", "svc"),
        ]));

        assert_eq!(out.len(), 1);
        assert_eq!(out.get("name"), Some(&json!("svc")));
    }

    #[test]
    fn maps_double_underscore_to_nesting() {
        let provider = EnvProvider::new();
        let out = provider.collect(vars(&[("APP_DATABASE__POOL__MAX", "20")]));
        assert_eq!(out.get("database"), Some(&json!({ "pool": { "max": 20 } })));
    }

    #[test]
    fn coerces_scalar_types() {
        let provider = EnvProvider::new();
        let out = provider.collect(vars(&[
            ("APP_DEBUG", "true"),
            ("APP_PORT", "8080"),
            ("APP_RATIO", "0.5"),
            ("APP_LABEL", "edge"),
        ]));
        assert_eq!(out.get("debug"), Some(&json!(true)));
        assert_eq!(out.get("port"), Some(&json!(8080)));
        assert_eq!(out.get("ratio"), Some(&json!(0.5)));
        assert_eq!(out.get("label"), Some(&json!("edge")));
    }

    #[test]
    fn custom_prefix() {
        let provider = EnvProvider::with_prefix("KLAUTHED");
        let out = provider.collect(vars(&[("KLAUTHED_NAME", "svc"), ("APP_NAME", "no")]));
        assert_eq!(out.len(), 1);
        assert_eq!(out.get("name"), Some(&json!("svc")));
    }
}
