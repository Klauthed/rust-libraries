#![deny(unsafe_code)]

//! Time as an injectable dependency.
//!
//! Instead of scattering `Utc::now()` through the code (which makes time-based
//! logic untestable), components take a [`Clock`]. Production wires
//! [`SystemClock`]; tests wire [`FixedClock`] to pin or advance time
//! deterministically.
//!
//! [`Timestamp`] is a thin newtype over [`chrono::DateTime<Utc>`] used as the
//! canonical instant type across the libraries.
//!
//! ```
//! use klauthed_core::time::{Clock, FixedClock, Timestamp};
//!
//! let clock = FixedClock::at_unix_millis(1_700_000_000_000);
//! let t0 = clock.now();
//! clock.advance(chrono::Duration::seconds(5));
//! assert_eq!(clock.now().duration_since(t0).num_seconds(), 5);
//! ```

use std::fmt;
use std::sync::Mutex;

use chrono::{DateTime, Duration, SecondsFormat, TimeZone, Utc};
use serde::{Deserialize, Serialize};

/// A point in time (UTC), the canonical instant type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Timestamp(DateTime<Utc>);

impl Timestamp {
    /// The current instant from the system clock.
    ///
    /// Prefer a [`Clock`] in code that should be testable; this is for the edges
    /// (e.g. constructing the [`SystemClock`] itself).
    pub fn now() -> Self {
        Self(Utc::now())
    }

    /// Wrap a chrono [`DateTime<Utc>`](chrono::DateTime).
    pub const fn from_datetime(dt: DateTime<Utc>) -> Self {
        Self(dt)
    }

    /// Construct from milliseconds since the Unix epoch.
    pub fn from_unix_millis(millis: i64) -> Self {
        Self(Utc.timestamp_millis_opt(millis).single().unwrap_or_else(Utc::now))
    }

    /// The underlying [`DateTime<Utc>`](chrono::DateTime).
    pub const fn as_datetime(&self) -> &DateTime<Utc> {
        &self.0
    }

    /// Consume into the underlying [`DateTime<Utc>`](chrono::DateTime).
    pub const fn into_datetime(self) -> DateTime<Utc> {
        self.0
    }

    /// Milliseconds since the Unix epoch.
    pub fn unix_millis(&self) -> i64 {
        self.0.timestamp_millis()
    }

    /// RFC 3339 / ISO 8601 representation (millisecond precision, `Z` suffix).
    pub fn to_rfc3339(&self) -> String {
        self.0.to_rfc3339_opts(SecondsFormat::Millis, true)
    }

    /// The signed duration elapsed since `earlier` (negative if `earlier` is later).
    pub fn duration_since(&self, earlier: Timestamp) -> Duration {
        self.0 - earlier.0
    }

    /// This instant shifted by `delta`.
    pub fn checked_add(&self, delta: Duration) -> Option<Timestamp> {
        self.0.checked_add_signed(delta).map(Timestamp)
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_rfc3339())
    }
}

impl From<DateTime<Utc>> for Timestamp {
    fn from(dt: DateTime<Utc>) -> Self {
        Self(dt)
    }
}

impl From<Timestamp> for DateTime<Utc> {
    fn from(ts: Timestamp) -> Self {
        ts.0
    }
}

/// A source of the current time.
///
/// Implementors are `Send + Sync` so a clock can be shared as `Arc<dyn Clock>`
/// across tasks.
pub trait Clock: Send + Sync {
    /// The current instant.
    fn now(&self) -> Timestamp;

    /// The current instant as a chrono [`DateTime<Utc>`](chrono::DateTime).
    fn now_datetime(&self) -> DateTime<Utc> {
        self.now().into_datetime()
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
    now: Mutex<DateTime<Utc>>,
}

impl FixedClock {
    /// A clock pinned to `at`.
    pub fn new(at: Timestamp) -> Self {
        Self {
            now: Mutex::new(at.into_datetime()),
        }
    }

    /// A clock pinned to `millis` since the Unix epoch.
    pub fn at_unix_millis(millis: i64) -> Self {
        Self::new(Timestamp::from_unix_millis(millis))
    }

    /// Reset the clock to `at`.
    pub fn set(&self, at: Timestamp) {
        *self.now.lock().expect("clock mutex poisoned") = at.into_datetime();
    }

    /// Move the clock forward (or backward, for a negative delta) by `delta`.
    pub fn advance(&self, delta: Duration) {
        let mut guard = self.now.lock().expect("clock mutex poisoned");
        *guard += delta;
    }
}

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        Timestamp::from_datetime(*self.now.lock().expect("clock mutex poisoned"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_round_trips_millis_and_rfc3339() {
        let ts = Timestamp::from_unix_millis(1_700_000_000_000);
        assert_eq!(ts.unix_millis(), 1_700_000_000_000);
        assert!(ts.to_rfc3339().ends_with('Z'));

        let json = serde_json::to_string(&ts).unwrap();
        let back: Timestamp = serde_json::from_str(&json).unwrap();
        assert_eq!(ts, back);
    }

    #[test]
    fn fixed_clock_is_deterministic_and_advanceable() {
        let clock = FixedClock::at_unix_millis(1_000);
        let t0 = clock.now();
        assert_eq!(clock.now(), t0); // does not move on its own

        clock.advance(Duration::seconds(5));
        assert_eq!(clock.now().duration_since(t0).num_seconds(), 5);

        clock.set(Timestamp::from_unix_millis(0));
        assert_eq!(clock.now().unix_millis(), 0);
    }

    #[test]
    fn works_behind_dyn_clock() {
        let clock: std::sync::Arc<dyn Clock> =
            std::sync::Arc::new(FixedClock::at_unix_millis(42));
        assert_eq!(clock.now().unix_millis(), 42);
    }

    #[test]
    fn system_clock_advances() {
        let clock = SystemClock;
        let a = clock.now();
        let b = clock.now();
        assert!(b >= a);
    }
}
