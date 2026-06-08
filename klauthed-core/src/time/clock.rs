//! The injectable [`Clock`] and its system / fixed implementations.

use std::sync::Mutex;

use time::{Duration, OffsetDateTime};

use super::Timestamp;

/// A source of the current time.
///
/// Implementors are `Send + Sync` so a clock can be shared as `Arc<dyn Clock>`
/// across tasks.
pub trait Clock: Send + Sync {
    /// The current instant.
    fn now(&self) -> Timestamp;

    /// The current instant as a [`time::OffsetDateTime`] (always UTC).
    fn now_datetime(&self) -> OffsetDateTime {
        self.now().into_offset_datetime()
    }
}

/// The real, system-backed clock for production use.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        Timestamp::now()
    }
}

/// A controllable clock for tests: pin time to a fixed instant and advance it
/// explicitly. Shareable through `&self`, so it works behind `Arc<dyn Clock>`.
#[derive(Debug)]
pub struct FixedClock {
    now: Mutex<Timestamp>,
}

impl FixedClock {
    /// A clock pinned to `at`.
    pub fn new(at: Timestamp) -> Self {
        Self { now: Mutex::new(at) }
    }

    /// A clock pinned to `millis` since the Unix epoch.
    pub fn at_unix_millis(millis: i64) -> Self {
        Self::new(Timestamp::from_unix_millis(millis))
    }

    /// Reset the clock to `at`.
    pub fn set(&self, at: Timestamp) {
        *self.now.lock().expect("clock mutex poisoned") = at;
    }

    /// Move the clock forward (or backward, for a negative delta) by `delta`.
    pub fn advance(&self, delta: Duration) {
        let mut guard = self.now.lock().expect("clock mutex poisoned");
        *guard =
            guard.checked_add(delta).expect("clock advance overflowed the representable range");
    }
}

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        *self.now.lock().expect("clock mutex poisoned")
    }
}
