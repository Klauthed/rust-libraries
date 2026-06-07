//! Public-API integration tests for the token denylist: revoke/is_revoked,
//! lazy expiry eviction, idempotent re-revocation, and shared cloned state.

use std::sync::Arc;

use klauthed_core::time::{Clock, Duration, FixedClock, Timestamp};
use klauthed_security::revocation::{InMemoryTokenDenylist, TokenDenylist};

fn denylist_at(millis: i64) -> (Arc<FixedClock>, InMemoryTokenDenylist) {
    let clock = Arc::new(FixedClock::at_unix_millis(millis));
    let list = InMemoryTokenDenylist::with_clock(clock.clone());
    (clock, list)
}

/// A timestamp roughly 10 years out — well within the representable range.
fn far_future() -> Timestamp {
    Timestamp::from_unix_millis(9_999_999_999_000) // ~year 2286
}

#[tokio::test]
async fn not_revoked_before_any_entry() {
    let (_clock, list) = denylist_at(0);
    assert!(!list.is_revoked("jti-1").await.unwrap());
}

#[tokio::test]
async fn revoked_token_is_detected() {
    let (_clock, list) = denylist_at(0);
    list.revoke("jti-1".into(), far_future()).await.unwrap();
    assert!(list.is_revoked("jti-1").await.unwrap());
}

#[tokio::test]
async fn other_jtis_are_unaffected() {
    let (_clock, list) = denylist_at(0);
    list.revoke("jti-a".into(), far_future()).await.unwrap();
    assert!(!list.is_revoked("jti-b").await.unwrap());
}

#[tokio::test]
async fn expired_entry_is_evicted_and_reported_as_not_revoked() {
    let (clock, list) = denylist_at(0);
    // Revoke with an expiry 30 seconds from now.
    let expires_at = clock.now().checked_add(Duration::seconds(30)).unwrap();
    list.revoke("jti-x".into(), expires_at).await.unwrap();

    assert!(list.is_revoked("jti-x").await.unwrap());
    assert_eq!(list.len(), 1);

    // Advance past the entry's expiry.
    clock.advance(Duration::seconds(31));

    assert!(!list.is_revoked("jti-x").await.unwrap());
    assert!(list.is_empty()); // lazily evicted
}

#[tokio::test]
async fn revoking_same_jti_twice_updates_expiry() {
    let (_clock, list) = denylist_at(0);
    let exp1 = Timestamp::from_unix_millis(1_000_000);
    let exp2 = far_future();

    list.revoke("jti".into(), exp1).await.unwrap();
    list.revoke("jti".into(), exp2).await.unwrap();

    assert_eq!(list.len(), 1);
    assert!(list.is_revoked("jti").await.unwrap());
}

#[tokio::test]
async fn cloned_lists_share_state() {
    let (_clock, list) = denylist_at(0);
    let clone = list.clone();

    list.revoke("jti".into(), far_future()).await.unwrap();
    assert!(clone.is_revoked("jti").await.unwrap());
}
