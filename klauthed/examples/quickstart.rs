//! Use klauthed through the umbrella crate. Each library is a feature-gated
//! module (`klauthed::core`, `klauthed::error`, …); this uses the default
//! `core` surface.
//!
//! Run with: `cargo run -p klauthed --example quickstart`

use klauthed::core::time::{Clock, Duration, FixedClock};
use klauthed::error::ErrorCategory;

fn main() {
    let clock = FixedClock::at_unix_millis(0);
    let started = clock.now();
    clock.advance(Duration::hours(2));
    println!(
        "elapsed: {}h (now {})",
        clock.now().duration_since(started).whole_hours(),
        clock.now(),
    );

    println!("not_found maps to HTTP {}", ErrorCategory::NotFound.http_status());
}
