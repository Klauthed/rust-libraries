//! The namespaced [`Permission`] string type and its wildcard matching.

/// A namespaced permission string, e.g. `"users:read"`.
///
/// By convention permissions are `"<resource>:<action>"`, but the type only
/// requires a string; matching treats `*` segments as wildcards. The two special
/// forms are `"*"` (everything) and `"<resource>:*"` (every action on a
/// resource).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Permission(String);

impl Permission {
    /// Wrap a permission string.
    pub fn new(perm: impl Into<String>) -> Self {
        Self(perm.into())
    }

    /// The permission as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether this (granted) permission covers `required`.
    ///
    /// Both sides are split on `:` into segments; a `*` segment in *this*
    /// permission matches any single segment in `required`, and a trailing `*`
    /// segment matches all remaining segments (so `"users:*"` covers
    /// `"users:read"`). A bare `"*"` covers everything.
    #[must_use]
    pub fn grants(&self, required: &Permission) -> bool {
        if self.0 == "*" {
            return true;
        }
        let granted: Vec<&str> = self.0.split(':').collect();
        let needed: Vec<&str> = required.0.split(':').collect();

        for (i, g) in granted.iter().enumerate() {
            // A trailing `*` segment swallows all remaining required segments,
            // but only if there is at least one to swallow (so `users:*` covers
            // `users:read` / `users:read:extra`, but not the bare `users`).
            if *g == "*" && i == granted.len() - 1 {
                return needed.len() > i;
            }
            match needed.get(i) {
                Some(n) if *g == "*" || g == n => {}
                _ => return false,
            }
        }
        // All granted segments matched; it's a grant only if there were no extra
        // required segments left over (i.e. equal length, exact match).
        granted.len() == needed.len()
    }
}

impl std::fmt::Display for Permission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for Permission {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for Permission {
    fn from(s: String) -> Self {
        Self(s)
    }
}
