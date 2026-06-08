//! IANA time-zone conversion for [`Timestamp`] (feature `tz`).
//!
//! [`Timestamp`] is always a UTC instant — the canonical, storage/compute form.
//! A [`TimeZone`] renders that same instant as *civil time* in a named zone
//! (e.g. `Europe/Istanbul`), applying the zone's UTC offset (including any DST
//! rule in effect at that instant). The instant itself never changes.
//!
//! ```
//! use klauthed_core::time::{Timestamp, TimeZone};
//!
//! let ts = Timestamp::from_unix_millis(1_700_000_000_000); // 2023-11-14T22:13:20Z
//! let tokyo = TimeZone::get("Asia/Tokyo").expect("known zone");
//!
//! // Same instant, expressed at Tokyo's +09:00 offset.
//! let local = ts.to_zone(&tokyo);
//! assert_eq!(local.offset().whole_hours(), 9);
//! assert_eq!(local.hour(), 7); // 22:13 UTC + 9h = 07:13 next day, Tokyo
//!
//! assert!(TimeZone::get("Mars/Olympus_Mons").is_none());
//! ```

use time::{OffsetDateTime, UtcOffset};
// `TimeZone` trait imported anonymously: brings `.name()` into scope without
// clashing with our own `TimeZone` type below.
use time_tz::{OffsetDateTimeExt, TimeZone as _, Tz, timezones};

use super::Timestamp;

/// A named IANA time zone (e.g. `Europe/Istanbul`).
///
/// Cheap to copy — it borrows a zone from the statically compiled tz database.
#[derive(Debug, Clone, Copy)]
pub struct TimeZone(&'static Tz);

impl TimeZone {
    /// Look up a zone by its IANA name, or `None` if the name is unknown.
    #[must_use]
    pub fn get(name: &str) -> Option<TimeZone> {
        timezones::get_by_name(name).map(TimeZone)
    }

    /// The zone's canonical IANA name.
    #[must_use]
    pub fn name(&self) -> &'static str {
        self.0.name()
    }
}

impl Timestamp {
    /// This instant rendered as civil time in `tz` — the same point in time,
    /// carrying the zone's UTC offset (with any DST rule applied for this
    /// instant). Returns a [`time::OffsetDateTime`]; the [`Timestamp`] itself
    /// stays UTC.
    #[must_use]
    pub fn to_zone(&self, tz: &TimeZone) -> OffsetDateTime {
        self.as_offset_datetime().to_timezone(tz.0)
    }

    /// The UTC offset in effect for this instant in `tz`.
    #[must_use]
    pub fn offset_in(&self, tz: &TimeZone) -> UtcOffset {
        self.to_zone(tz).offset()
    }
}
