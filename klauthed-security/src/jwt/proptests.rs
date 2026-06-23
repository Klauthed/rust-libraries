//! Property tests for JWT signing / verification round-trips.

use klauthed_core::time::{Duration, SystemClock};
use proptest::prelude::*;

use super::{Claims, JwtSigner, JwtVerifier};

const SECRET: &[u8] = b"property-test-hs256-secret-32-bytes!";

proptest! {
    // Encoding then verifying preserves every claim exactly (HS256). The verifier
    // also enforces the issuer/audience it was built with, so this covers the
    // positive validation path too.
    #[test]
    fn hs256_round_trips_all_claims(
        sub in "[a-zA-Z0-9._-]{1,40}",
        iss in "[a-zA-Z0-9._-]{1,40}",
        aud in "[a-zA-Z0-9._-]{1,40}",
        role in "[a-zA-Z0-9._ -]{0,40}",
    ) {
        let claims = Claims::builder(sub.as_str(), &SystemClock, Duration::hours(1))
            .issuer(iss.as_str())
            .audience(aud.as_str())
            .claim("role", role.as_str())
            .build();

        let token = JwtSigner::hs256(SECRET).encode(&claims).unwrap();
        let decoded = JwtVerifier::hs256(SECRET)
            .expecting_issuer(iss.as_str())
            .expecting_audience(aud.as_str())
            .decode(&token)
            .unwrap();

        prop_assert_eq!(decoded, claims);
    }

    // A token signed with one secret must not verify under a different secret.
    #[test]
    fn a_different_key_is_rejected(sub in "[a-zA-Z0-9._-]{1,40}") {
        let claims = Claims::builder(sub.as_str(), &SystemClock, Duration::hours(1)).build();
        let token = JwtSigner::hs256(SECRET).encode(&claims).unwrap();
        let result = JwtVerifier::hs256(b"a-totally-different-hs256-secret!").decode(&token);
        prop_assert!(result.is_err());
    }

    // A token whose `exp` is in the past is rejected.
    #[test]
    fn an_expired_token_is_rejected(sub in "[a-zA-Z0-9._-]{1,40}") {
        let claims = Claims::builder(sub.as_str(), &SystemClock, Duration::hours(-1)).build();
        let token = JwtSigner::hs256(SECRET).encode(&claims).unwrap();
        prop_assert!(JwtVerifier::hs256(SECRET).decode(&token).is_err());
    }
}
