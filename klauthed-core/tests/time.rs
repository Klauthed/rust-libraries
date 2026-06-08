//! Public-API integration tests for the time module: `Timestamp` conversions,
//! the injectable `Clock`, and (with the `tz` feature) zone rendering.

use klauthed_core::time::{Clock, Duration, FixedClock, SystemClock, Timestamp};
use time::PrimitiveDateTime;

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
    let clock: std::sync::Arc<dyn Clock> = std::sync::Arc::new(FixedClock::at_unix_millis(42));
    assert_eq!(clock.now().unix_millis(), 42);
}

#[test]
fn system_clock_advances() {
    let clock = SystemClock;
    let a = clock.now();
    let b = clock.now();
    assert!(b >= a);
}

#[cfg(feature = "tz")]
#[test]
fn timezone_renders_civil_time_in_a_named_zone() {
    use klauthed_core::time::TimeZone;
    use time::UtcOffset;

    // 2023-11-14T22:13:20Z.
    let ts = Timestamp::from_unix_millis(1_700_000_000_000);

    // Tokyo is a stable UTC+09:00 (no DST).
    let tokyo = TimeZone::get("Asia/Tokyo").expect("known zone");
    assert_eq!(tokyo.name(), "Asia/Tokyo");
    assert_eq!(ts.offset_in(&tokyo), UtcOffset::from_hms(9, 0, 0).unwrap());

    let local = ts.to_zone(&tokyo);
    assert_eq!(local.offset().whole_hours(), 9);
    assert_eq!(local.hour(), 7); // 22:13 UTC + 9h rolls into the next morning
    // The instant is unchanged — only its rendered offset differs.
    assert_eq!(local.unix_timestamp(), ts.unix_seconds());

    // A DST zone resolves the correct offset for the instant (Istanbul = +03:00).
    let istanbul = TimeZone::get("Europe/Istanbul").expect("known zone");
    assert_eq!(ts.offset_in(&istanbul), UtcOffset::from_hms(3, 0, 0).unwrap());

    assert!(TimeZone::get("Mars/Olympus_Mons").is_none());
}
