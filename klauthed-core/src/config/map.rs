//! [`ConfigMap`] — the currency of the config engine.
//!
//! Every provider yields a `ConfigMap` and the builder merges them into one. It
//! is a thin newtype over `BTreeMap<String, Value>` that carries the two
//! tree-shaping operations as methods, so call sites read fluently:
//!
//! ```
//! use klauthed_core::config::ConfigMap;
//! use serde_json::json;
//!
//! let nested = ConfigMap::from_iter([("database.url".to_string(), json!("postgres://x"))])
//!     .expand_dotted();
//! assert_eq!(nested.get("database"), Some(&json!({ "url": "postgres://x" })));
//! ```

use std::collections::BTreeMap;

use serde_json::{Map, Value};

/// A (possibly nested) map of top-level config keys to JSON values.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigMap(BTreeMap<String, Value>);

impl ConfigMap {
    /// An empty map.
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// Consume the wrapper, returning the inner `BTreeMap`.
    pub fn into_inner(self) -> BTreeMap<String, Value> {
        self.0
    }

    /// Borrow the inner `BTreeMap`.
    pub fn as_map(&self) -> &BTreeMap<String, Value> {
        &self.0
    }

    /// Number of top-level keys.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the map has no top-level keys.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Get a top-level value by exact key.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.get(key)
    }

    /// Insert a top-level key, returning any previous value.
    pub fn insert(&mut self, key: impl Into<String>, value: Value) -> Option<Value> {
        self.0.insert(key.into(), value)
    }

    /// Iterate over the top-level keys.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.0.keys().map(String::as_str)
    }

    /// Expand flat, dotted keys into nested objects, consuming `self`.
    ///
    /// `{"database.url": "...", "database.pool.max": 10}` becomes
    /// `{"database": {"url": "...", "pool": {"max": 10}}}`. Keys without a `.`
    /// are carried over unchanged.
    pub fn expand_dotted(self) -> ConfigMap {
        let mut root = ConfigMap::new();
        for (key, value) in self.0 {
            match key.split_once('.') {
                None => merge_value(root.0.entry(key).or_insert(Value::Null), value),
                Some((head, tail)) => {
                    let entry =
                        root.0.entry(head.to_owned()).or_insert_with(|| Value::Object(Map::new()));
                    insert_nested(entry, tail, value);
                }
            }
        }
        root
    }

    /// Deep-merge `other` on top of `self`: keys present only in `other` are
    /// inserted; keys in both are merged recursively, with `other` winning on
    /// scalar/array conflicts. This is how a later provider overrides an earlier
    /// one without discarding sibling keys.
    pub fn merge(&mut self, other: ConfigMap) {
        for (key, value) in other.0 {
            match self.0.get_mut(&key) {
                Some(existing) => merge_value(existing, value),
                None => {
                    self.0.insert(key, value);
                }
            }
        }
    }
}

impl From<BTreeMap<String, Value>> for ConfigMap {
    fn from(map: BTreeMap<String, Value>) -> Self {
        Self(map)
    }
}

impl From<ConfigMap> for BTreeMap<String, Value> {
    fn from(map: ConfigMap) -> Self {
        map.0
    }
}

impl FromIterator<(String, Value)> for ConfigMap {
    fn from_iter<I: IntoIterator<Item = (String, Value)>>(iter: I) -> Self {
        Self(BTreeMap::from_iter(iter))
    }
}

impl IntoIterator for ConfigMap {
    type Item = (String, Value);
    type IntoIter = std::collections::btree_map::IntoIter<String, Value>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

/// Insert `value` at the dotted `path` within `target`, creating intermediate
/// objects as needed.
fn insert_nested(target: &mut Value, path: &str, value: Value) {
    if !target.is_object() {
        *target = Value::Object(Map::new());
    }
    #[allow(clippy::expect_used, reason = "target was set to an Object on the lines above")]
    let obj = target.as_object_mut().expect("ensured object above");

    match path.split_once('.') {
        None => merge_value(obj.entry(path.to_owned()).or_insert(Value::Null), value),
        Some((head, tail)) => {
            let child = obj.entry(head.to_owned()).or_insert_with(|| Value::Object(Map::new()));
            insert_nested(child, tail, value);
        }
    }
}

/// Deep-merge `overlay` into `base`.
///
/// * Two objects are merged key-by-key (recursively).
/// * Anything else — scalars, arrays, or a type change — replaces `base`
///   wholesale. Arrays are intentionally *not* concatenated: a later layer that
///   sets a list fully replaces the earlier one, which is the least surprising
///   behavior for things like seed hosts or allowed origins.
pub(crate) fn merge_value(base: &mut Value, overlay: Value) {
    match (base, overlay) {
        (Value::Object(base_map), Value::Object(overlay_map)) => {
            for (key, value) in overlay_map {
                match base_map.get_mut(&key) {
                    Some(existing) => merge_value(existing, value),
                    None => {
                        base_map.insert(key, value);
                    }
                }
            }
        }
        (slot, overlay) => *slot = overlay,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn expands_dotted_keys_into_nested_objects() {
        let map = ConfigMap::from_iter([
            ("database.url".to_string(), json!("postgres://x")),
            ("database.pool.max".to_string(), json!(10)),
            ("app_name".to_string(), json!("svc")),
        ]);

        let nested = map.expand_dotted();

        assert_eq!(nested.get("app_name"), Some(&json!("svc")));
        assert_eq!(
            nested.get("database"),
            Some(&json!({ "url": "postgres://x", "pool": { "max": 10 } }))
        );
    }

    #[test]
    fn merge_recurses_objects_and_overrides_scalars() {
        let mut base = ConfigMap::from_iter([
            ("database".to_string(), json!({ "host": "localhost", "port": 5432 })),
            ("debug".to_string(), json!(false)),
        ]);
        base.merge(ConfigMap::from_iter([
            ("database".to_string(), json!({ "port": 6543, "user": "svc" })),
            ("debug".to_string(), json!(true)),
            ("extra".to_string(), json!("x")),
        ]));

        assert_eq!(
            base.get("database"),
            Some(&json!({ "host": "localhost", "port": 6543, "user": "svc" }))
        );
        assert_eq!(base.get("debug"), Some(&json!(true)));
        assert_eq!(base.get("extra"), Some(&json!("x")));
    }

    #[test]
    fn merge_replaces_arrays_wholesale() {
        let mut base = ConfigMap::from_iter([("hosts".to_string(), json!(["a", "b", "c"]))]);
        base.merge(ConfigMap::from_iter([("hosts".to_string(), json!(["d"]))]));
        assert_eq!(base.get("hosts"), Some(&json!(["d"])));
    }
}
