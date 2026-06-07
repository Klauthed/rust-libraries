//! Public-API integration test for the test-clock helpers.

use klauthed_testing::clock::{Clock, Duration, fixed_clock};

#[test]
fn fixed_clock_pins_and_advances_deterministically() {
    let clock = fixed_clock(1_000);
    let t0 = clock.now();
    assert_eq!(clock.now(), t0); // does not move on its own

    clock.advance(Duration::seconds(5));
    assert_eq!(clock.now().duration_since(t0).whole_seconds(), 5);
}
