//! `klauthed-core`: injectable time (+ timezones), validation, and typed ids.

use klauthed_core::id::Id;
use klauthed_core::time::{Clock, Duration, FixedClock, TimeZone, Timestamp};
use klauthed_core::validation::{Validate, ValidationErrors};

// A zero-sized marker so `Id<User>` and `Id<Order>` are distinct types.
struct User;
type UserId = Id<User>;

struct SignUp {
    email: String,
    age: u8,
}

impl Validate for SignUp {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        if !self.email.contains('@') {
            errors.add("email", "invalid_email", "must be a valid email address");
        }
        if self.age < 18 {
            errors.add("age", "too_small", "must be at least 18");
        }
        errors.into_result()
    }
}

pub fn run() {
    // Injectable clock: pin "now" and advance it deterministically.
    let clock = FixedClock::at_unix_millis(1_700_000_000_000);
    let t0 = clock.now();
    clock.advance(Duration::hours(2));
    let elapsed = clock.now().duration_since(t0);
    println!("  clock advanced by {}h", elapsed.whole_hours());
    assert_eq!(elapsed.whole_hours(), 2);

    // The instant is UTC; render it as civil time in a named zone (tz feature).
    let ts = Timestamp::from_unix_millis(1_700_000_000_000); // 2023-11-14T22:13:20Z
    let tokyo = TimeZone::get("Asia/Tokyo").expect("known zone");
    let local = ts.to_zone(&tokyo);
    println!("  {ts} is {:02}:{:02} local in {}", local.hour(), local.minute(), tokyo.name());
    assert_eq!(ts.offset_in(&tokyo).whole_hours(), 9);

    // Typed ids: distinct per marker, round-trip through their string form.
    let id = UserId::new();
    let parsed: UserId = id.to_string().parse().expect("round-trips");
    println!("  minted UserId {id} (round-trips: {})", parsed == id);
    assert_eq!(parsed, id);

    // Validation accumulates every problem.
    let errors = SignUp { email: "nope".into(), age: 12 }.validate().unwrap_err();
    println!("  SignUp validation reported {} problem(s)", errors.len());
    assert_eq!(errors.len(), 2);
    assert!(SignUp { email: "a@b.co".into(), age: 21 }.validate().is_ok());
}
