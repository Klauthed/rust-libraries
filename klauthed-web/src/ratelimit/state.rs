//! The shared fixed-window counter [`State`] and its per-key [`Decision`].

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// One client's counter within the current window.
#[derive(Debug, Clone, Copy)]
struct Window {
    /// When the current window started.
    started: Instant,
    /// Requests seen in the current window.
    count: u32,
}

/// Shared limiter state: a counter per client key.
#[derive(Debug, Default)]
pub(crate) struct State {
    windows: Mutex<HashMap<String, Window>>,
}

/// Outcome of recording one request against a key.
pub(crate) enum Decision {
    /// Allowed; nothing more to do.
    Allowed,
    /// Rejected; retry after the given duration.
    Limited { retry_after: Duration },
}

impl State {
    /// Record a request for `key`, returning whether it is allowed.
    pub(crate) fn check(&self, key: &str, max: u32, window: Duration, now: Instant) -> Decision {
        let mut windows = self.windows.lock().expect("rate-limit mutex poisoned");
        let entry = windows.entry(key.to_owned()).or_insert(Window { started: now, count: 0 });

        // Reset the window if it has elapsed.
        if now.duration_since(entry.started) >= window {
            entry.started = now;
            entry.count = 0;
        }

        if entry.count >= max {
            let elapsed = now.duration_since(entry.started);
            let retry_after = window.saturating_sub(elapsed);
            Decision::Limited { retry_after }
        } else {
            entry.count += 1;
            Decision::Allowed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[std::prelude::v1::test]
    fn fixed_window_allows_then_limits_then_resets() {
        let state = State::default();
        let window = Duration::from_secs(10);
        let t0 = Instant::now();

        // First two allowed, third limited.
        assert!(matches!(state.check("k", 2, window, t0), Decision::Allowed));
        assert!(matches!(state.check("k", 2, window, t0), Decision::Allowed));
        assert!(matches!(state.check("k", 2, window, t0), Decision::Limited { .. }));

        // After the window elapses, the budget refreshes.
        let t1 = t0 + window;
        assert!(matches!(state.check("k", 2, window, t1), Decision::Allowed));
    }

    #[std::prelude::v1::test]
    fn keys_are_independent() {
        let state = State::default();
        let window = Duration::from_secs(10);
        let now = Instant::now();
        assert!(matches!(state.check("a", 1, window, now), Decision::Allowed));
        assert!(matches!(state.check("a", 1, window, now), Decision::Limited { .. }));
        // A different key has its own fresh budget.
        assert!(matches!(state.check("b", 1, window, now), Decision::Allowed));
    }
}
