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

    /// Construct from milliseconds since the Unix epoch, or `None` if `millis`
    /// falls outside the representable range (roughly years ±262144).
    ///
    /// Mirrors chrono's `timestamp_millis_opt`. Prefer this over
    /// [`from_unix_millis`](Self::from_unix_millis) when `millis` is untrusted
    /// or computed and an out-of-range value should be treated as an error
    /// rather than silently clamped.
    pub fn from_unix_millis_opt(millis: i64) -> Option<Self> {
        Utc.timestamp_millis_opt(millis).single().map(Self)
    }

    /// Construct from milliseconds since the Unix epoch.
    ///
    /// **Saturating:** a `millis` value outside the representable range (roughly
    /// years ±262144) is clamped to the earliest or latest representable
    /// instant, *preserving order* — a far-future overflow stays in the far
    /// future and a far-past underflow stays in the far past; neither collapses
    /// to "now". Use [`from_unix_millis_opt`](Self::from_unix_millis_opt) to
    /// detect out-of-range input instead of saturating.
    pub fn from_unix_millis(millis: i64) -> Self {
        // Saturate toward the sign of the input so the result keeps the same
        // ordering relative to "now" that the caller intended.
        let saturated = if millis >= 0 {
            Self(DateTime::<Utc>::MAX_UTC)
        } else {
            Self(DateTime::<Utc>::MIN_UTC)
        };
        Self::from_unix_millis_opt(millis).unwrap_or(saturated)
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
    fn from_unix_millis_opt_rejects_out_of_range() {
        assert!(Timestamp::from_unix_millis_opt(0).is_some());
        assert!(Timestamp::from_unix_millis_opt(1_700_000_000_000).is_some());
        // Beyond chrono's representable range.
        assert!(Timestamp::from_unix_millis_opt(i64::MAX).is_none());
        assert!(Timestamp::from_unix_millis_opt(i64::MIN).is_none());
    }

    #[test]
    fn from_unix_millis_saturates_instead_of_collapsing_to_now() {
        let now = Timestamp::now();

        // A far-future overflow must stay in the far future, not become "now".
        let future = Timestamp::from_unix_millis(i64::MAX);
        assert!(future > now);
        assert_eq!(future, Timestamp::from_datetime(DateTime::<Utc>::MAX_UTC));

        // A far-past underflow saturates to the earliest representable instant.
        let past = Timestamp::from_unix_millis(i64::MIN);
        assert!(past < now);
        assert_eq!(past, Timestamp::from_datetime(DateTime::<Utc>::MIN_UTC));
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
