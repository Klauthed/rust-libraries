//! Drive a deterministic clock in tests/fixtures.
//!
//! Run with: `cargo run -p klauthed-testing --example fixed_clock`

use klauthed_testing::clock::{Clock, Duration, fixed_clock};

fn main() {
    let clock = fixed_clock(0);
    println!("t0 = {}", clock.now());

    clock.advance(Duration::hours(1));
    println!("t1 = {} (advanced 1h)", clock.now());

    clock.advance(Duration::minutes(-30));
    println!("t2 = {} (rewound 30m)", clock.now());
}
