//! The phantom-typed [`Id`] type.

use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::str::FromStr;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

use super::ParseIdError;

/// A typed, 128-bit identifier backed by a [`Uuid`].
///
/// The phantom `T` is a compile-time tag only; it carries no data and imposes no
/// bounds, so any type (usually a zero-sized marker) can stand in for it.
pub struct Id<T: ?Sized> {
    value: Uuid,
    // `fn() -> T` keeps `Id<T>: Send + Sync + Copy` regardless of `T`, and makes
    // the phantom covariant without tying `T` to drop/auto-trait behavior.
    _marker: PhantomData<fn() -> T>,
}

impl<T: ?Sized> Id<T> {
    /// Wrap an existing [`Uuid`].
    pub const fn from_uuid(value: Uuid) -> Self {
        Self { value, _marker: PhantomData }
    }

    /// The nil (all-zero) id, useful as a sentinel.
    pub const fn nil() -> Self {
        Self::from_uuid(Uuid::nil())
    }

    /// Generate a new id with the default strategy (**UUID v7**, time-sortable).
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self::new_v7()
    }

    /// Generate a time-sortable **UUID v7** id (default).
    pub fn new_v7() -> Self {
        Self::from_uuid(Uuid::now_v7())
    }

    /// Generate a random **UUID v4** id (no embedded time; good for opaque keys).
    pub fn new_v4() -> Self {
        Self::from_uuid(Uuid::new_v4())
    }

    /// Generate a **ULID**-based id, stored as the same 128 bits in a [`Uuid`].
    pub fn new_ulid() -> Self {
        Self::from_uuid(Uuid::from_u128(ulid::Ulid::new().into()))
    }

    /// The underlying [`Uuid`].
    pub const fn as_uuid(&self) -> &Uuid {
        &self.value
    }

    /// Consume into the underlying [`Uuid`].
    pub const fn into_uuid(self) -> Uuid {
        self.value
    }

    /// Whether this is the nil (all-zero) id.
    pub fn is_nil(&self) -> bool {
        self.value.is_nil()
    }

    /// Render as a ULID (Crockford base32) string.
    pub fn to_ulid_string(&self) -> String {
        ulid::Ulid::from(self.value.as_u128()).to_string()
    }

    /// Parse from a ULID (Crockford base32) string.
    pub fn from_ulid_str(s: &str) -> Result<Self, ParseIdError> {
        ulid::Ulid::from_string(s)
            .map(|u| Self::from_uuid(Uuid::from_u128(u.into())))
            .map_err(|_| ParseIdError::new(s))
    }
}

// ── Manual trait impls (deriving would wrongly require `T: Trait`) ─────────────

impl<T: ?Sized> Clone for Id<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: ?Sized> Copy for Id<T> {}

impl<T: ?Sized> PartialEq for Id<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<T: ?Sized> Eq for Id<T> {}

impl<T: ?Sized> PartialOrd for Id<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T: ?Sized> Ord for Id<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.value.cmp(&other.value)
    }
}

impl<T: ?Sized> Hash for Id<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}

impl<T: ?Sized> fmt::Debug for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Id({})", self.value)
    }
}

impl<T: ?Sized> fmt::Display for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.value, f)
    }
}

impl<T: ?Sized> FromStr for Id<T> {
    type Err = ParseIdError;

    /// Accepts either a UUID or a ULID string.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(uuid) = Uuid::parse_str(s) {
            return Ok(Self::from_uuid(uuid));
        }
        if let Ok(ulid) = ulid::Ulid::from_string(s) {
            return Ok(Self::from_uuid(Uuid::from_u128(ulid.into())));
        }
        Err(ParseIdError::new(s))
    }
}

impl<T: ?Sized> Serialize for Id<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Canonical hyphenated UUID — the most DB/tool-compatible form.
        self.value.serialize(serializer)
    }
}

impl<'de, T: ?Sized> Deserialize<'de> for Id<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // String-oriented: accept either UUID or ULID text.
        let raw = String::deserialize(deserializer)?;
        raw.parse().map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    // A throwaway marker for the phantom type parameter.
    struct Thing;

    proptest! {
        /// Display → `FromStr` round-trips through the canonical UUID form.
        #[test]
        fn uuid_string_round_trips(n in any::<u128>()) {
            let id = Id::<Thing>::from_uuid(Uuid::from_u128(n));
            let parsed: Id<Thing> = id.to_string().parse().unwrap();
            prop_assert_eq!(id, parsed);
        }

        /// `to_ulid_string` → `from_ulid_str` round-trips the same 128 bits.
        #[test]
        fn ulid_string_round_trips(n in any::<u128>()) {
            let id = Id::<Thing>::from_uuid(Uuid::from_u128(n));
            let parsed = Id::<Thing>::from_ulid_str(&id.to_ulid_string()).unwrap();
            prop_assert_eq!(id, parsed);
        }

        /// `FromStr` accepts both encodings of the same id and yields equal ids.
        #[test]
        fn uuid_and_ulid_forms_agree(n in any::<u128>()) {
            let id = Id::<Thing>::from_uuid(Uuid::from_u128(n));
            let from_uuid: Id<Thing> = id.to_string().parse().unwrap();
            let from_ulid: Id<Thing> = id.to_ulid_string().parse().unwrap();
            prop_assert_eq!(from_uuid, from_ulid);
        }

        /// Id ordering tracks the underlying 128-bit value (stable across stores).
        #[test]
        fn ordering_matches_u128(a in any::<u128>(), b in any::<u128>()) {
            let ia = Id::<Thing>::from_uuid(Uuid::from_u128(a));
            let ib = Id::<Thing>::from_uuid(Uuid::from_u128(b));
            prop_assert_eq!(ia.cmp(&ib), a.cmp(&b));
        }

        /// Parsing rejects non-id text instead of panicking.
        #[test]
        fn arbitrary_text_never_panics(s in ".*") {
            let _ = s.parse::<Id<Thing>>();
        }
    }
}
