//! Public-API tests for the session value types ([`SessionId`], [`Session`]):
//! id generation/wrapping/display and the inclusive expiry boundary.

use std::collections::HashMap;

use klauthed_core::time::Timestamp;
use klauthed_security::session::{Session, SessionId};

#[test]
fn generated_ids_are_unique_and_nonempty() {
    let a = SessionId::generate().unwrap();
    let b = SessionId::generate().unwrap();
    assert_ne!(a, b, "each generated id must be distinct");
    assert!(!a.as_str().is_empty());
}

#[test]
fn token_round_trips_through_str_display_and_owned() {
    let id = SessionId::from_token("tok-123");
    assert_eq!(id.as_str(), "tok-123");
    assert_eq!(id.to_string(), "tok-123");
    assert_eq!(id.clone().into_string(), "tok-123");
}

#[test]
fn expiry_boundary_is_inclusive() {
    let expires = Timestamp::from_unix_millis(2_000);
    let session = Session {
        id: SessionId::from_token("sid"),
        subject: "user-1".into(),
        created_at: Timestamp::from_unix_millis(1_000),
        expires_at: expires,
        metadata: HashMap::from([("ip".into(), "10.0.0.1".into())]),
    };

    assert!(!session.is_expired(Timestamp::from_unix_millis(1_999)));
    // Inclusive: a session is expired at exactly `expires_at`.
    assert!(session.is_expired(expires));
    assert!(session.is_expired(Timestamp::from_unix_millis(2_001)));
}
