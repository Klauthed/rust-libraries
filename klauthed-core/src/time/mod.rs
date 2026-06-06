#![deny(unsafe_code)]

//! Time as an injectable dependency.
//!
//! Instead of scattering `now()` calls through the code (which makes time-based
//! logic untestable), components take a [`Clock`]. Production wires
//! [`SystemClock`]; tests wire [`FixedClock`] to pin or advance time
//! deterministically.
//!
//! [`Timestamp`] is the canonical instant type across the libraries and
//! [`Duration`] the canonical span type. Both are backed by the [`time`] crate,
//! but that is an implementation detail: depend on `klauthed_core::time`, not on
//! `time` directly, so the backing library stays swappable.
//!
//! ```
//! use klauthed_core::time::{Clock, Duration, FixedClock};
//!
//! let clock = FixedClock::at_unix_millis(1_700_000_000_000);
//! let t0 = clock.now();
//! clock.advance(Duration::seconds(5));
//! assert_eq!(clock.now().duration_since(t0).whole_seconds(), 5);
//! ```

use std::fmt;
use std::sync::Mutex;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use time::format_description::well_known::Rfc3339;
use time::{OffsetDateTime, PrimitiveDateTime, UtcOffset};

/// A span of time (TTLs, deltas), the canonical duration type.
///
/// Re-exported from the backing datetime library so callers depend on
/// `klauthed_core::time::Duration` rather than the concrete crate.
pub use time::Duration;

/// A point in time (UTC), the canonical instant type.
///
/// Serializes as a millisecond-precision RFC 3339 string with a `Z` UTC
/// designator (e.g. `2023-11-14T22:13:20.000Z`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(OffsetDateTime);

impl Timestamp {
    /// The current instant from the system clock.
    ///
    /// Prefer a [`Clock`] in code that should be testable; this is for the edges
    /// (e.g. constructing the [`SystemClock`] itself).
    pub fn now() -> Self {
        Self(OffsetDateTime::now_utc())
    }

    /// Wrap a [`time::OffsetDateTime`], normalising it to UTC.
    pub fn from_offset_datetime(dt: OffsetDateTime) -> Self {
        Self(dt.to_offset(UtcOffset::UTC))
    }

    /// The underlying [`time::OffsetDateTime`] (always UTC).
    pub const fn as_offset_datetime(&self) -> &OffsetDateTime {
        &self.0
    }

    /// Consume into the underlying [`time::OffsetDateTime`] (always UTC).
    pub const fn into_offset_datetime(self) -> OffsetDateTime {
        self.0
    }

    /// Construct from milliseconds since the Unix epoch, or `None` if `millis`
    /// falls outside the representable range (roughly years ±9999).
    ///
    /// Prefer this over [`from_unix_millis`](Self::from_unix_millis) when
    /// `millis` is untrusted or computed and an out-of-range value should be
    /// treated as an error rather than silently clamped.
    pub fn from_unix_millis_opt(millis: i64) -> Option<Self> {
        OffsetDateTime::from_unix_timestamp_nanos(millis as i128 * 1_000_000)
            .ok()
            .map(Self)
    }

    /// Construct from milliseconds since the Unix epoch.
    ///
    /// **Saturating:** a `millis` value outside the representable range is
    /// clamped to the earliest or latest representable instant, *preserving
    /// order* — a far-future overflow stays in the far future and a far-past
    /// underflow stays in the far past; neither collapses to "now". Use
    /// [`from_unix_millis_opt`](Self::from_unix_millis_opt) to detect
    /// out-of-range input instead of saturating.
    pub fn from_unix_millis(millis: i64) -> Self {
        Self::from_unix_millis_opt(millis).unwrap_or(Self::saturated(millis >= 0))
    }

    /// Construct from seconds since the Unix epoch, or `None` if `secs` falls
    /// outside the representable range.
    pub fn from_unix_seconds_opt(secs: i64) -> Option<Self> {
        OffsetDateTime::from_unix_timestamp(secs).ok().map(Self)
    }

    /// Construct from seconds since the Unix epoch, saturating on out-of-range
    /// input (see [`from_unix_millis`](Self::from_unix_millis)).
    pub fn from_unix_seconds(secs: i64) -> Self {
        Self::from_unix_seconds_opt(secs).unwrap_or(Self::saturated(secs >= 0))
    }

    /// The latest or earliest representable instant, used as the saturation
    /// target for out-of-range conversions.
    fn saturated(non_negative: bool) -> Self {
        if non_negative {
            Self(PrimitiveDateTime::MAX.assume_utc())
        } else {
            Self(PrimitiveDateTime::MIN.assume_utc())
        }
    }

    /// Milliseconds since the Unix epoch.
    pub fn unix_millis(&self) -> i64 {
        (self.0.unix_timestamp_nanos() / 1_000_000) as i64
    }

    /// Whole seconds since the Unix epoch.
    pub fn unix_seconds(&self) -> i64 {
        self.0.unix_timestamp()
    }

    /// RFC 3339 / ISO 8601 representation (millisecond precision, `Z` suffix).
    pub fn to_rfc3339(&self) -> String {
        // Fixed format matches the historical wire contract: UTC `Z` designator
        // with exactly three subsecond digits.
        let fmt = time::macros::format_description!(
            "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z"
        );
        self.0
            .format(fmt)
            .expect("formatting a UTC timestamp with a fixed description cannot fail")
    }

    /// The signed duration elapsed since `earlier` (negative if `earlier` is later).
    pub fn duration_since(&self, earlier: Timestamp) -> Duration {
        self.0 - earlier.0
    }

    /// This instant shifted by `delta`, or `None` on over/underflow.
    pub fn checked_add(&self, delta: Duration) -> Option<Timestamp> {
        self.0.checked_add(delta).map(Timestamp)
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_rfc3339())
    }
}

impl From<OffsetDateTime> for Timestamp {
    fn from(dt: OffsetDateTime) -> Self {
        Self::from_offset_datetime(dt)
    }
}

impl From<Timestamp> for OffsetDateTime {
    fn from(ts: Timestamp) -> Self {
        ts.0
    }
}

impl Serialize for Timestamp {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Full-precision RFC 3339 (`Z`, subsecond digits as needed) so
        // serialization is lossless and round-trips exactly. `to_rfc3339` is the
        // millisecond-precision *human* format and is intentionally separate.
        let s = self.0.format(&Rfc3339).map_err(serde::ser::Error::custom)?;
        serializer.serialize_str(&s)
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        OffsetDateTime::parse(&s, &Rfc3339)
            .map(Self::from_offset_datetime)
            .map_err(serde::de::Error::custom)
    }
}

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
        *guard = guard
            .checked_add(delta)
            .expect("clock advance overflowed the representable range");
    }
}

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        *self.now.lock().expect("clock mutex poisoned")
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
        // Beyond the representable range.
        assert!(Timestamp::from_unix_millis_opt(i64::MAX).is_none());
        assert!(Timestamp::from_unix_millis_opt(i64::MIN).is_none());
    }

    #[test]
    fn from_unix_millis_saturates_instead_of_collapsing_to_now() {
        let now = Timestamp::now();

        // A far-future overflow must stay in the far future, not become "now".
        let future = Timestamp::from_unix_millis(i64::MAX);
        assert!(future > now);
        assert_eq!(future, Timestamp::from_offset_datetime(PrimitiveDateTime::MAX.assume_utc()));

        // A far-past underflow saturates to the earliest representable instant.
        let past = Timestamp::from_unix_millis(i64::MIN);
        assert!(past < now);
        assert_eq!(past, Timestamp::from_offset_datetime(PrimitiveDateTime::MIN.assume_utc()));
    }

    #[test]
    fn unix_seconds_matches_millis() {
        let ts = Timestamp::from_unix_millis(1_700_000_000_000);
        assert_eq!(ts.unix_seconds(), 1_700_000_000);
    }

    #[test]
    fn fixed_clock_is_deterministic_and_advanceable() {
        let clock = FixedClock::at_unix_millis(1_000);
        let t0 = clock.now();
        assert_eq!(clock.now(), t0); // does not move on its own

        clock.advance(Duration::seconds(5));
        assert_eq!(clock.now().duration_since(t0).whole_seconds(), 5);

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
