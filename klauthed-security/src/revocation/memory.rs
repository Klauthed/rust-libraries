//! The [`InMemoryTokenDenylist`]: a clock-injected `Mutex<HashMap>` denylist.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use klauthed_core::time::{Clock, SystemClock, Timestamp};

use super::TokenDenylist;
use crate::error::SecurityError;

/// An in-memory [`TokenDenylist`] backed by a `Mutex<HashMap>`.
///
/// Cloneable handles share the same backing map (`Arc<Mutex<…>>`). Inject a
/// [`FixedClock`](klauthed_core::time::FixedClock) in tests to control expiry
/// deterministically.
///
/// Expired entries are lazily evicted on each [`is_revoked`] call to prevent
/// unbounded growth. For production deployments with many short-lived tokens,
/// prefer a Redis-backed implementation that handles TTL natively.
///
/// [`is_revoked`]: InMemoryTokenDenylist::is_revoked
#[derive(Clone)]
pub struct InMemoryTokenDenylist {
    entries: Arc<Mutex<HashMap<String, Timestamp>>>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for InMemoryTokenDenylist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let len = self.entries.lock().map(|m| m.len()).unwrap_or(0);
        f.debug_struct("InMemoryTokenDenylist").field("entries", &len).finish_non_exhaustive()
    }
}

impl Default for InMemoryTokenDenylist {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryTokenDenylist {
    /// A denylist backed by the real system clock.
    #[must_use]
    pub fn new() -> Self {
        Self::with_clock(Arc::new(SystemClock))
    }

    /// A denylist driven by `clock`.
    #[must_use]
    pub fn with_clock(clock: Arc<dyn Clock>) -> Self {
        Self { entries: Arc::new(Mutex::new(HashMap::new())), clock }
    }

    /// Number of entries currently in the denylist (including expired ones not
    /// yet evicted).
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.lock().expect("denylist mutex poisoned").len()
    }

    /// Whether the denylist has no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.lock().expect("denylist mutex poisoned").is_empty()
    }
}

#[async_trait]
impl TokenDenylist for InMemoryTokenDenylist {
    async fn revoke(&self, jti: String, expires_at: Timestamp) -> Result<(), SecurityError> {
        self.entries.lock().expect("denylist mutex poisoned").insert(jti, expires_at);
        Ok(())
    }

    async fn is_revoked(&self, jti: &str) -> Result<bool, SecurityError> {
        let now = self.clock.now();
        let mut map = self.entries.lock().expect("denylist mutex poisoned");
        match map.get(jti).copied() {
            Some(expires_at) if expires_at <= now => {
                // The entry has expired — lazily evict and report as not revoked.
                map.remove(jti);
                Ok(false)
            }
            Some(_) => Ok(true),
            None => Ok(false),
        }
    }
}
