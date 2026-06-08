//! Public-API integration tests for typed identifiers: uniqueness, UUID/ULID
//! round-trips, parse errors, ordering, and serde.

use klauthed_core::id::Id;
use klauthed_error::{DomainError, ErrorCategory};

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
