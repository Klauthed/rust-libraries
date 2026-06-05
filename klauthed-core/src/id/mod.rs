#![deny(unsafe_code)]

//! Typed 128-bit identifiers.
//!
//! [`Id<T>`] is a phantom-typed newtype over a [`Uuid`], so `Id<User>` and
//! `Id<Order>` are distinct types the compiler refuses to mix. The default
//! generator is **UUID v7** (time-sortable); v4 and ULID generation are also
//! available. Because all three encode the same 128 bits, one id can be rendered
//! and parsed as either a UUID or a ULID string.
//!
//! ```
//! use klauthed_core::id::Id;
//!
//! struct User;
//! type UserId = Id<User>;
//!
//! let a = UserId::new();        // UUID v7, time-sortable
//! let b = UserId::new();
//! assert!(a != b);
//! // round-trips through both string forms:
//! assert_eq!(a, a.to_string().parse().unwrap());
//! assert_eq!(a, UserId::from_ulid_str(&a.to_ulid_string()).unwrap());
//! ```

use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::str::FromStr;

use klauthed_error::{DomainError, ErrorCategory, ErrorCode};
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

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
        Self {
            value,
            _marker: PhantomData,
        }
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

/// Error returned when a string is neither a valid UUID nor ULID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseIdError {
    input: String,
}

impl ParseIdError {
    fn new(input: &str) -> Self {
        Self {
            input: input.to_owned(),
        }
    }
}

impl fmt::Display for ParseIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "'{}' is not a valid UUID or ULID", self.input)
    }
}

impl std::error::Error for ParseIdError {}

impl DomainError for ParseIdError {
    fn category(&self) -> ErrorCategory {
        ErrorCategory::BadRequest
    }

    fn code(&self) -> ErrorCode {
        ErrorCode::new("id.invalid")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Distinct marker types to prove the compile-time separation.
    struct User;
    struct Order;
    type UserId = Id<User>;
    type OrderId = Id<Order>;

    #[test]
    fn generates_unique_sortable_v7_ids() {
        let a = UserId::new();
        let b = UserId::new();
        assert_ne!(a, b);
        // v7 ids generated later sort at or after earlier ones.
        assert!(b >= a);
    }

    #[test]
    fn uuid_and_ulid_string_forms_round_trip() {
        let id = UserId::new();
        assert_eq!(id, id.to_string().parse::<UserId>().unwrap());
        assert_eq!(id, UserId::from_ulid_str(&id.to_ulid_string()).unwrap());
    }

    #[test]
    fn from_str_accepts_both_encodings() {
        let id = UserId::new();
        let from_uuid: UserId = id.to_string().parse().unwrap();
        let from_ulid: UserId = id.to_ulid_string().parse().unwrap();
        assert_eq!(from_uuid, id);
        assert_eq!(from_ulid, id);
    }

    #[test]
    fn invalid_string_is_a_bad_request_domain_error() {
        let err = "not-an-id".parse::<UserId>().unwrap_err();
        assert_eq!(err.category(), ErrorCategory::BadRequest);
        assert_eq!(err.code().as_str(), "id.invalid");
    }

    #[test]
    fn serde_uses_string_form() {
        let id = UserId::new();
        let json = serde_json::to_string(&id).unwrap();
        assert!(json.starts_with('"'));
        let back: UserId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }

    #[test]
    fn v4_and_ulid_generators_work() {
        assert!(!UserId::new_v4().is_nil());
        assert!(!UserId::new_ulid().is_nil());
        assert!(UserId::nil().is_nil());
    }

    #[test]
    fn different_marker_types_are_distinct_but_same_layout() {
        let u = UserId::new();
        // Same bytes, re-tagged — only possible via explicit conversion.
        let o = OrderId::from_uuid(u.into_uuid());
        assert_eq!(u.as_uuid(), o.as_uuid());
    }
}
