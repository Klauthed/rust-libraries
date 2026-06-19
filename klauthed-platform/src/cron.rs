//! A small, dependency-light **cron** expression parser and next-occurrence
//! calculator, built on the `time` crate (no chrono).
//!
//! Standard 5-field syntax — `minute hour day-of-month month day-of-week` — with
//! `*`, single values, ranges (`a-b`), lists (`a,b,c`), and steps (`*/n`,
//! `a-b/n`). Day-of-week is `0..=6` with Sunday `0` (`7` is also accepted as
//! Sunday). Schedules are evaluated in **UTC** by default, or in a named IANA
//! timezone via [`Cron::parse_in_timezone`] (DST is handled correctly).
//!
//! Day-of-month / day-of-week follow the usual cron rule: if both are restricted
//! (neither is `*`), a day matches when **either** matches; if one is `*`, only
//! the other constrains the day.
//!
//! ```
//! use klauthed_platform::scheduler::Cron;
//! let every_15m = Cron::parse("*/15 * * * *").unwrap();
//! ```

use std::fmt;

use klauthed_core::time::{TimeZone, Timestamp};
use time::{Duration, OffsetDateTime, UtcOffset};

/// An invalid cron expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CronError(String);

impl fmt::Display for CronError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid cron expression: {}", self.0)
    }
}

impl std::error::Error for CronError {}

/// One cron field: a bitmask of allowed values plus whether it was a bare `*`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Field {
    mask: u64,
    is_star: bool,
}

impl Field {
    fn contains(self, value: u8) -> bool {
        value < 64 && (self.mask & (1u64 << value)) != 0
    }
}

/// A parsed 5-field cron schedule, evaluated in UTC or a named timezone.
#[derive(Debug, Clone)]
pub struct Cron {
    minute: Field,
    hour: Field,
    dom: Field,
    month: Field,
    dow: Field,
    /// The zone schedule fields are evaluated in; `None` means UTC.
    tz: Option<TimeZone>,
}

impl Cron {
    /// Parse a 5-field cron expression, evaluated in **UTC**.
    ///
    /// # Errors
    /// Returns [`CronError`] if the expression doesn't have five fields or a
    /// field is malformed or out of range.
    pub fn parse(expr: &str) -> Result<Self, CronError> {
        Self::parse_fields(expr, None)
    }

    /// Parse a 5-field cron expression evaluated in the named IANA `timezone`
    /// (e.g. `"America/New_York"`), so e.g. `"0 9 * * *"` fires at 09:00 local
    /// time year-round, across DST transitions.
    ///
    /// # Errors
    /// Returns [`CronError`] if the expression is malformed or `timezone` is not
    /// a known IANA zone name.
    pub fn parse_in_timezone(expr: &str, timezone: &str) -> Result<Self, CronError> {
        let tz = TimeZone::get(timezone)
            .ok_or_else(|| CronError(format!("unknown timezone '{timezone}'")))?;
        Self::parse_fields(expr, Some(tz))
    }

    fn parse_fields(expr: &str, tz: Option<TimeZone>) -> Result<Self, CronError> {
        let parts: Vec<&str> = expr.split_whitespace().collect();
        let [minute, hour, dom, month, dow] = parts.as_slice() else {
            return Err(CronError(format!("expected 5 fields, got {}", parts.len())));
        };
        Ok(Cron {
            minute: parse_field(minute, 0, 59, false)?,
            hour: parse_field(hour, 0, 23, false)?,
            dom: parse_field(dom, 1, 31, false)?,
            month: parse_field(month, 1, 12, false)?,
            dow: parse_field(dow, 0, 7, true)?,
            tz,
        })
    }

    /// The next instant **strictly after** `after` that matches this schedule,
    /// truncated to the minute, or `None` if none occurs within ~5 years (e.g. an
    /// impossible expression like February 30th).
    pub fn next_after(&self, after: OffsetDateTime) -> Option<OffsetDateTime> {
        // Start at the next whole minute.
        let mut t =
            after.replace_second(0).ok()?.replace_nanosecond(0).ok()? + Duration::minutes(1);
        const MAX_MINUTES: u32 = 5 * 366 * 24 * 60;
        for _ in 0..MAX_MINUTES {
            if self.matches(t) {
                return Some(t);
            }
            t += Duration::minutes(1);
        }
        None
    }

    /// The civil time `t` is matched against, in the schedule's zone (UTC by
    /// default). Stepping happens in absolute time, so converting each candidate
    /// here makes DST transitions fall out naturally.
    fn local(&self, t: OffsetDateTime) -> OffsetDateTime {
        match &self.tz {
            Some(tz) => Timestamp::from_offset_datetime(t).to_zone(tz),
            None => t.to_offset(UtcOffset::UTC),
        }
    }

    fn matches(&self, t: OffsetDateTime) -> bool {
        let t = self.local(t);
        self.minute.contains(t.minute())
            && self.hour.contains(t.hour())
            && self.month.contains(u8::from(t.month()))
            && self.day_matches(t)
    }

    fn day_matches(&self, t: OffsetDateTime) -> bool {
        let dom_ok = self.dom.contains(t.day());
        let dow_ok = self.dow.contains(t.weekday().number_days_from_sunday());
        match (self.dom.is_star, self.dow.is_star) {
            (true, true) => true,
            (false, true) => dom_ok,
            (true, false) => dow_ok,
            (false, false) => dom_ok || dow_ok,
        }
    }
}

/// Parse one field over `[min, max]`. With `wrap` (day-of-week), values are taken
/// modulo 7 so `7` means Sunday (`0`).
fn parse_field(spec: &str, min: u8, max: u8, wrap: bool) -> Result<Field, CronError> {
    let is_star = spec == "*";
    let mut mask = 0u64;

    for term in spec.split(',') {
        let (range, step) = match term.split_once('/') {
            Some((range, step_str)) => {
                let step: u8 = step_str
                    .parse()
                    .map_err(|_| CronError(format!("invalid step '{step_str}'")))?;
                if step == 0 {
                    return Err(CronError("step must be greater than zero".into()));
                }
                (range, step)
            }
            None => (term, 1),
        };

        let (lo, hi) = if range == "*" {
            (min, max)
        } else if let Some((a, b)) = range.split_once('-') {
            (parse_num(a, min, max)?, parse_num(b, min, max)?)
        } else {
            let v = parse_num(range, min, max)?;
            (v, v)
        };
        if lo > hi {
            return Err(CronError(format!("range start {lo} is after end {hi}")));
        }

        let mut v = lo;
        while v <= hi {
            let bit = if wrap { v % 7 } else { v };
            mask |= 1u64 << bit;
            v += step;
        }
    }

    Ok(Field { mask, is_star })
}

fn parse_num(s: &str, min: u8, max: u8) -> Result<u8, CronError> {
    let v: u8 = s.trim().parse().map_err(|_| CronError(format!("'{s}' is not a number")))?;
    if v < min || v > max {
        return Err(CronError(format!("{v} is out of range {min}-{max}")));
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn rejects_malformed_expressions() {
        assert!(Cron::parse("* * * *").is_err()); // 4 fields
        assert!(Cron::parse("* * * * * *").is_err()); // 6 fields
        assert!(Cron::parse("60 * * * *").is_err()); // minute out of range
        assert!(Cron::parse("* 24 * * *").is_err()); // hour out of range
        assert!(Cron::parse("* * 0 * *").is_err()); // day-of-month < 1
        assert!(Cron::parse("* * * 13 *").is_err()); // month > 12
        assert!(Cron::parse("*/0 * * * *").is_err()); // zero step
        assert!(Cron::parse("5-1 * * * *").is_err()); // reversed range
        assert!(Cron::parse("x * * * *").is_err()); // not a number
    }

    #[test]
    fn accepts_common_forms() {
        for expr in ["* * * * *", "0 0 * * *", "*/15 * * * *", "0 9-17 * * 1-5", "0,30 * 1,15 * *"]
        {
            assert!(Cron::parse(expr).is_ok(), "{expr} should parse");
        }
    }

    #[test]
    fn every_minute_advances_one_minute() {
        let cron = Cron::parse("* * * * *").unwrap();
        let now = datetime!(2026-06-19 12:30:45 UTC);
        assert_eq!(cron.next_after(now), Some(datetime!(2026-06-19 12:31:00 UTC)));
    }

    #[test]
    fn hourly_goes_to_the_next_top_of_hour() {
        let cron = Cron::parse("0 * * * *").unwrap();
        let now = datetime!(2026-06-19 12:30:00 UTC);
        assert_eq!(cron.next_after(now), Some(datetime!(2026-06-19 13:00:00 UTC)));
    }

    #[test]
    fn daily_at_a_specific_time() {
        let cron = Cron::parse("30 9 * * *").unwrap();
        // Before 09:30 → same day.
        assert_eq!(
            cron.next_after(datetime!(2026-06-19 08:00:00 UTC)),
            Some(datetime!(2026-06-19 09:30:00 UTC))
        );
        // After 09:30 → next day.
        assert_eq!(
            cron.next_after(datetime!(2026-06-19 10:00:00 UTC)),
            Some(datetime!(2026-06-20 09:30:00 UTC))
        );
    }

    #[test]
    fn step_every_quarter_hour() {
        let cron = Cron::parse("*/15 * * * *").unwrap();
        assert_eq!(
            cron.next_after(datetime!(2026-06-19 12:31:00 UTC)),
            Some(datetime!(2026-06-19 12:45:00 UTC))
        );
        assert_eq!(
            cron.next_after(datetime!(2026-06-19 12:50:00 UTC)),
            Some(datetime!(2026-06-19 13:00:00 UTC))
        );
    }

    #[test]
    fn day_of_week_monday() {
        // 2026-06-19 is a Friday; next Monday 00:00 is 2026-06-22.
        let cron = Cron::parse("0 0 * * 1").unwrap();
        assert_eq!(
            cron.next_after(datetime!(2026-06-19 12:00:00 UTC)),
            Some(datetime!(2026-06-22 00:00:00 UTC))
        );
    }

    #[test]
    fn sunday_accepts_zero_and_seven() {
        let zero = Cron::parse("0 0 * * 0").unwrap();
        let seven = Cron::parse("0 0 * * 7").unwrap();
        let now = datetime!(2026-06-19 12:00:00 UTC); // Friday
        // 2026-06-21 is the next Sunday.
        let expected = Some(datetime!(2026-06-21 00:00:00 UTC));
        assert_eq!(zero.next_after(now), expected);
        assert_eq!(seven.next_after(now), expected);
    }

    #[test]
    fn first_of_next_month() {
        let cron = Cron::parse("0 0 1 * *").unwrap();
        assert_eq!(
            cron.next_after(datetime!(2026-06-19 12:00:00 UTC)),
            Some(datetime!(2026-07-01 00:00:00 UTC))
        );
    }

    #[test]
    fn dom_and_dow_are_or_when_both_restricted() {
        // "1st of the month OR a Monday" at 00:00.
        let cron = Cron::parse("0 0 1 * 1").unwrap();
        // From Fri 2026-06-19: next Monday (06-22) comes before the 1st (07-01).
        assert_eq!(
            cron.next_after(datetime!(2026-06-19 12:00:00 UTC)),
            Some(datetime!(2026-06-22 00:00:00 UTC))
        );
    }

    #[test]
    fn impossible_expression_returns_none() {
        let cron = Cron::parse("0 0 30 2 *").unwrap(); // February 30th
        assert_eq!(cron.next_after(datetime!(2026-01-01 00:00:00 UTC)), None);
    }

    #[test]
    fn unknown_timezone_is_rejected() {
        assert!(Cron::parse_in_timezone("* * * * *", "Mars/Phobos").is_err());
        assert!(Cron::parse_in_timezone("0 12 * * *", "America/New_York").is_ok());
    }

    #[test]
    fn timezone_cron_fires_at_local_time_across_dst() {
        // "noon in New York" — the resulting UTC instant differs by DST.
        let cron = Cron::parse_in_timezone("0 12 * * *", "America/New_York").unwrap();

        // Summer: EDT (UTC-4) → 12:00 local is 16:00 UTC.
        assert_eq!(
            cron.next_after(datetime!(2026-06-19 08:00:00 UTC)),
            Some(datetime!(2026-06-19 16:00:00 UTC))
        );
        // Winter: EST (UTC-5) → 12:00 local is 17:00 UTC.
        assert_eq!(
            cron.next_after(datetime!(2026-01-15 08:00:00 UTC)),
            Some(datetime!(2026-01-15 17:00:00 UTC))
        );
    }
}
