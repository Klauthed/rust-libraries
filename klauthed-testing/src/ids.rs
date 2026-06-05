//! Deterministic identifiers for test fixtures.
//!
//! Production ids ([`Id::new`](klauthed_core::id::Id::new)) are random/time-based
//! and so differ on every run, which makes assertions and golden files brittle.
//! These helpers mint **stable** ids from a `u64` seed: the same seed always
//! yields the same [`Id<T>`], so fixtures are reproducible across runs and across
//! marker types.

use klauthed_core::id::Id;
use uuid::Uuid;

/// A reproducible [`Id<T>`] derived from the seed `n`.
///
/// The same `n` always produces the same id (for any marker type `T`), and
/// distinct `n` values produce distinct ids. The seed is placed in the low 64
/// bits of the underlying [`Uuid`], so the rendered id is easy to recognize at a
/// glance (e.g. seed `7` → `...-0000000000000007`).
///
/// This is deliberately *not* a valid versioned UUID; it is a test fixture id,
/// not a generated production id.
///
/// ```
/// use klauthed_testing::ids::seeded_id;
/// use klauthed_core::id::Id;
///
/// struct User;
/// type UserId = Id<User>;
///
/// // Stable across calls...
/// assert_eq!(seeded_id::<User>(7), seeded_id::<User>(7));
/// // ...and distinct across seeds.
/// assert_ne!(seeded_id::<User>(7), seeded_id::<User>(8));
/// // The seed is visible in the low bits.
/// assert!(seeded_id::<User>(7).to_string().ends_with("000000000007"));
/// ```
pub fn seeded_id<T: ?Sized>(n: u64) -> Id<T> {
    Id::from_uuid(Uuid::from_u128(u128::from(n)))
}

/// The nil (all-zero) [`Id<T>`], useful as a sentinel or "unset" fixture value.
///
/// ```
/// use klauthed_testing::ids::nil_id;
/// use klauthed_core::id::Id;
///
/// struct Order;
/// assert!(nil_id::<Order>().is_nil());
/// ```
pub fn nil_id<T: ?Sized>() -> Id<T> {
    Id::nil()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct User;
    struct Order;

    #[test]
    fn seeded_ids_are_stable_and_distinct() {
        assert_eq!(seeded_id::<User>(1), seeded_id::<User>(1));
        assert_ne!(seeded_id::<User>(1), seeded_id::<User>(2));
    }

    #[test]
    fn seed_is_in_the_low_bits() {
        let id = seeded_id::<User>(0xABCD);
        assert_eq!(id.as_uuid().as_u128(), 0xABCD);
    }

    #[test]
    fn same_seed_across_marker_types_shares_bytes() {
        let u = seeded_id::<User>(99);
        let o = seeded_id::<Order>(99);
        assert_eq!(u.as_uuid(), o.as_uuid());
    }

    #[test]
    fn nil_id_is_nil() {
        assert!(nil_id::<User>().is_nil());
        assert_eq!(seeded_id::<User>(0), nil_id::<User>());
    }
}
