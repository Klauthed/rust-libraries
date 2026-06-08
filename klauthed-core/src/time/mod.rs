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

pub mod clock;
pub mod timestamp;
#[cfg(feature = "tz")]
pub mod zone;

pub use clock::{Clock, FixedClock, SystemClock};
pub use timestamp::Timestamp;
#[cfg(feature = "tz")]
pub use zone::TimeZone;

/// A span of time (TTLs, deltas), the canonical duration type.
///
/// Re-exported from the backing datetime library so callers depend on
/// `klauthed_core::time::Duration` rather than the concrete crate.
pub use time::Duration;
