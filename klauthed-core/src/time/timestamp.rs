//! The canonical UTC instant type, [`Timestamp`].

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime, PrimitiveDateTime, UtcOffset};

/// A point in time (UTC), the canonical instant type.
///
/// Serializes as a millisecond-precision RFC 3339 string with a `Z` UTC
/// designator (e.g. `2023-11-14T22:13:20.000Z`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(OffsetDateTime);

impl Timestamp {
    /// The current instant from the system clock.
    ///
    /// Prefer a [`Clock`](super::Clock) in code that should be testable; this is for the edges
    /// (e.g. constructing the [`SystemClock`](super::SystemClock) itself).
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
        OffsetDateTime::from_unix_timestamp_nanos(millis as i128 * 1_000_000).ok().map(Self)
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
    #[allow(
        clippy::expect_used,
        reason = "formatting an in-range UTC value with a static format description is infallible"
    )]
    pub fn to_rfc3339(&self) -> String {
        // Fixed format matches the historical wire contract: UTC `Z` designator
        // with exactly three subsecond digits.
        let fmt = time::macros::format_description!(
            "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z"
        );
        self.0.format(fmt).expect("formatting a UTC timestamp with a fixed description cannot fail")
    }

    /// Parse an RFC 3339 / ISO 8601 timestamp (e.g. from [`to_rfc3339`]), or
    /// `None` if it is malformed. The inverse of [`to_rfc3339`].
    ///
    /// [`to_rfc3339`]: Timestamp::to_rfc3339
    #[must_use]
    pub fn parse_rfc3339(s: &str) -> Option<Self> {
        OffsetDateTime::parse(s, &Rfc3339).map(Self::from_offset_datetime).ok()
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

#[cfg(test)]
mod proptests {
    use super::Timestamp;
    use proptest::prelude::*;
    use time::Duration;

    // Millis comfortably inside the representable range (~years 1716..2223), so
    // conversions never saturate and round-trips are exact.
    const LO: i64 = -8_000_000_000_000;
    const HI: i64 = 8_000_000_000_000;

    proptest! {
        #[test]
        fn millis_round_trip(m in LO..HI) {
            prop_assert_eq!(Timestamp::from_unix_millis_opt(m).unwrap().unix_millis(), m);
        }

        #[test]
        fn seconds_round_trip(s in (LO / 1000)..(HI / 1000)) {
            prop_assert_eq!(Timestamp::from_unix_seconds_opt(s).unwrap().unix_seconds(), s);
        }

        #[test]
        fn ordering_matches_millis(a in LO..HI, b in LO..HI) {
            let (ta, tb) = (Timestamp::from_unix_millis(a), Timestamp::from_unix_millis(b));
            prop_assert_eq!(ta <= tb, a <= b);
            prop_assert_eq!(ta.cmp(&tb), a.cmp(&b));
        }

        #[test]
        fn duration_since_is_the_millis_difference(a in LO..HI, b in LO..HI) {
            let (ta, tb) = (Timestamp::from_unix_millis(a), Timestamp::from_unix_millis(b));
            prop_assert_eq!(
                tb.duration_since(ta).whole_milliseconds(),
                i128::from(b) - i128::from(a)
            );
        }

        #[test]
        fn checked_add_then_duration_since_returns_the_delta(
            m in LO..HI,
            delta_ms in -1_000_000_000i64..1_000_000_000,
        ) {
            let t = Timestamp::from_unix_millis(m);
            let shifted = t.checked_add(Duration::milliseconds(delta_ms)).unwrap();
            prop_assert_eq!(shifted.duration_since(t).whole_milliseconds(), i128::from(delta_ms));
        }

        // `to_rfc3339` is millisecond-precision; built from millis it round-trips.
        #[test]
        fn rfc3339_human_form_round_trips(m in LO..HI) {
            let t = Timestamp::from_unix_millis(m);
            prop_assert_eq!(Timestamp::parse_rfc3339(&t.to_rfc3339()), Some(t));
        }

        // serde is full-precision and lossless.
        #[test]
        fn serde_round_trips(m in LO..HI) {
            let t = Timestamp::from_unix_millis(m);
            let json = serde_json::to_string(&t).unwrap();
            prop_assert_eq!(serde_json::from_str::<Timestamp>(&json).unwrap(), t);
        }

        // Out-of-range conversions saturate while preserving order (documented).
        #[test]
        fn saturation_preserves_order(m in LO..HI) {
            let t = Timestamp::from_unix_millis(m);
            prop_assert!(Timestamp::from_unix_millis(i64::MAX) >= t);
            prop_assert!(Timestamp::from_unix_millis(i64::MIN) <= t);
        }
    }
}
