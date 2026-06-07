//! Integration test for the umbrella crate's feature-gated re-exports.
//!
//! Uses only the default (`core`, which implies `error`) feature surface.

use klauthed::core::time::{Clock, Duration, FixedClock};
use klauthed::error::ErrorCategory;

#[test]
fn reexports_core_and_error() {
    // `klauthed::core` re-exports klauthed-core.
    let clock = FixedClock::at_unix_millis(0);
    let t0 = clock.now();
    clock.advance(Duration::minutes(90));
    assert_eq!(clock.now().duration_since(t0).whole_minutes(), 90);

    // `klauthed::error` re-exports the error kernel.
    assert_eq!(ErrorCategory::NotFound.http_status(), 404);
}
