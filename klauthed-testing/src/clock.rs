//! Clock helpers for tests.
//!
//! These are thin conveniences around [`klauthed_core::time::FixedClock`], the
//! controllable test clock. Construct one pinned to a known instant, then pin or
//! [`advance`](klauthed_core::time::FixedClock::advance) it explicitly so
//! time-dependent logic is fully deterministic.
//!
//! [`Clock`] and [`Timestamp`] are re-exported here so tests can pull everything
//! time-related from one place.

pub use klauthed_core::time::{Clock, Duration, FixedClock, Timestamp};

/// A [`FixedClock`] pinned to `unix_millis` milliseconds since the Unix epoch.
///
/// A terse alias for [`FixedClock::at_unix_millis`] so tests read clearly:
///
/// ```
/// use klauthed_testing::clock::{fixed_clock, Clock, Duration};
///
/// let clock = fixed_clock(1_700_000_000_000);
/// let t0 = clock.now();
/// clock.advance(Duration::seconds(5));
/// assert_eq!(clock.now().duration_since(t0).whole_seconds(), 5);
/// ```
pub fn fixed_clock(unix_millis: i64) -> FixedClock {
    FixedClock::at_unix_millis(unix_millis)
}

/// A [`FixedClock`] pinned to the Unix epoch (`0` ms).
///
/// Handy when the absolute instant is irrelevant and you only care about
/// relative progress via [`advance`](FixedClock::advance).
pub fn epoch_clock() -> FixedClock {
    fixed_clock(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_clock_pins_and_advances() {
        let clock = fixed_clock(1_000);
        let t0 = clock.now();
        assert_eq!(t0.unix_millis(), 1_000);
        assert_eq!(clock.now(), t0); // does not move on its own

        clock.advance(Duration::seconds(2));
        assert_eq!(clock.now().duration_since(t0).whole_seconds(), 2);
    }

    #[test]
    fn epoch_clock_starts_at_zero() {
        let clock = epoch_clock();
        assert_eq!(clock.now().unix_millis(), 0);
    }

    #[test]
    fn usable_behind_dyn_clock() {
        let clock: std::sync::Arc<dyn Clock> = std::sync::Arc::new(fixed_clock(42));
        assert_eq!(clock.now().unix_millis(), 42);
    }
}
