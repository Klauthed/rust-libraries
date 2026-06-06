//! Tests for the JWK and JWK Set types.

use crate::ProtocolError;

use super::*;

const RSA_JWKS: &str = r#"{
        "keys": [
            {
                "kty": "RSA",
                "use": "sig",
                "kid": "abc",
                "alg": "RS256",
                "n": "0vx7agoebGcQSuuPiLJXZptN9nndrQmbXEps2aiAFbWhM78LhWx",
                "e": "AQAB"
            }
        ]
    }"#;

#[test]
fn rsa_jwks_round_trips_with_exact_keys() {
    let set: JsonWebKeySet = serde_json::from_str(RSA_JWKS).unwrap();
    assert_eq!(set.keys.len(), 1);
    let key = &set.keys[0];
    assert_eq!(key.kty, KeyType::Rsa);
    assert_eq!(key.key_use, Some(PublicKeyUse::Signature));
    assert_eq!(key.kid.as_deref(), Some("abc"));
    assert_eq!(key.alg.as_deref(), Some("RS256"));
    assert_eq!(key.e.as_deref(), Some("AQAB"));
    assert!(key.n.is_some());

    let json = serde_json::to_value(&set).unwrap();
    let k = &json["keys"][0];
    // Exact wire keys.
    assert_eq!(k["kty"], "RSA");
    assert_eq!(k["use"], "sig");
    assert_eq!(k["kid"], "abc");
    assert_eq!(k["alg"], "RS256");
    assert_eq!(k["e"], "AQAB");
    assert!(k.get("n").is_some());
    // Unset key-type material for other types is omitted.
    assert!(k.get("crv").is_none());
    assert!(k.get("x").is_none());
    assert!(k.get("y").is_none());
    assert!(k.get("k").is_none());
    assert!(k.get("x5c").is_none());
    assert!(k.get("x5t").is_none());
    assert!(k.get("key_ops").is_none());
    // No leaked internal field names.
    assert!(k.get("additional").is_none());
    assert!(k.get("key_use").is_none());
}

#[test]
fn find_hit_and_miss() {
    let set: JsonWebKeySet = serde_json::from_str(RSA_JWKS).unwrap();
    assert!(set.find("abc").is_some());
    assert!(set.find("nope").is_none());
}

#[test]
fn require_returns_error_on_miss() {
    let set: JsonWebKeySet = serde_json::from_str(RSA_JWKS).unwrap();
    assert!(set.require("abc").is_ok());
    let err = set.require("missing").unwrap_err();
    match err {
        ProtocolError::KeyNotFound { kid } => assert_eq!(kid, "missing"),
        other => panic!("expected KeyNotFound, got {other:?}"),
    }
}

#[test]
fn ec_key_round_trips() {
    let json = r#"{
            "kty": "EC",
            "use": "sig",
            "kid": "ec-1",
            "crv": "P-256",
            "x": "f83OJ3D2xF1Bg8vub9tLe1gHMzV76e8Tus9uPHvRVEU",
            "y": "x_FEzRu9m36HLN_tue659LNpXW6pCyStikYjKIWI5a0"
        }"#;
    let key: JsonWebKey = serde_json::from_str(json).unwrap();
    assert_eq!(key.kty, KeyType::Ec);
    assert_eq!(key.kind(), Some(KeyType::Ec));
    assert_eq!(key.crv.as_deref(), Some("P-256"));
    assert!(key.x.is_some());
    assert!(key.y.is_some());
    assert!(key.n.is_none());

    let reser = serde_json::to_value(&key).unwrap();
    assert_eq!(reser["kty"], "EC");
    assert_eq!(reser["crv"], "P-256");
    assert!(reser.get("y").is_some());
    assert!(reser.get("n").is_none());
}

#[test]
fn oct_key_round_trips_and_preserves_unmodeled() {
    let json = r#"{
            "kty": "oct",
            "kid": "sym-1",
            "alg": "HS256",
            "k": "GawgguFyGrWKav7AX4VKUg",
            "vendor_meta": {"rotated": true}
        }"#;
    let key: JsonWebKey = serde_json::from_str(json).unwrap();
    assert_eq!(key.kty, KeyType::Oct);
    assert_eq!(key.k.as_deref(), Some("GawgguFyGrWKav7AX4VKUg"));
    assert_eq!(key.key_use, None);
    // Unmodeled members preserved verbatim.
    assert_eq!(key.additional["vendor_meta"]["rotated"], true);

    let reser = serde_json::to_value(&key).unwrap();
    assert_eq!(reser["kty"], "oct");
    assert_eq!(reser["k"], "GawgguFyGrWKav7AX4VKUg");
    assert_eq!(reser["vendor_meta"]["rotated"], true);
}

#[test]
fn private_rsa_material_lands_in_additional() {
    let json = r#"{
            "kty": "RSA",
            "kid": "priv",
            "n": "0vx7",
            "e": "AQAB",
            "d": "X4cTteJ",
            "p": "83i-7I",
            "q": "3dfOR9c"
        }"#;
    let key: JsonWebKey = serde_json::from_str(json).unwrap();
    assert!(key.additional.contains_key("d"));
    assert!(key.additional.contains_key("p"));
    assert!(key.additional.contains_key("q"));
    let reser = serde_json::to_value(&key).unwrap();
    assert_eq!(reser["d"], "X4cTteJ");
}

#[test]
fn signing_key_predicate() {
    let sig = JsonWebKey {
        kty: KeyType::Rsa,
        key_use: Some(PublicKeyUse::Signature),
        ..Default::default()
    };
    let unset = JsonWebKey { kty: KeyType::Rsa, key_use: None, ..Default::default() };
    let enc = JsonWebKey {
        kty: KeyType::Rsa,
        key_use: Some(PublicKeyUse::Encryption),
        ..Default::default()
    };
    assert!(sig.is_signing_key());
    assert!(unset.is_signing_key());
    assert!(!enc.is_signing_key());
}

fn multi_key_set() -> JsonWebKeySet {
    JsonWebKeySet {
        keys: vec![
            JsonWebKey {
                kty: KeyType::Oct,
                kid: Some("sym".into()),
                alg: Some("HS256".into()),
                k: Some("secret".into()),
                ..Default::default()
            },
            JsonWebKey {
                kty: KeyType::Rsa,
                key_use: Some(PublicKeyUse::Encryption),
                kid: Some("enc".into()),
                alg: Some("RSA-OAEP".into()),
                ..Default::default()
            },
            JsonWebKey {
                kty: KeyType::Rsa,
                key_use: Some(PublicKeyUse::Signature),
                kid: Some("rs".into()),
                alg: Some("RS256".into()),
                ..Default::default()
            },
            JsonWebKey {
                kty: KeyType::Ec,
                key_use: None,
                kid: Some("es".into()),
                alg: Some("ES256".into()),
                ..Default::default()
            },
        ],
    }
}

#[test]
fn find_signing_keys_excludes_oct_and_enc() {
    let set = multi_key_set();
    let kids: Vec<_> = set.find_signing_keys().filter_map(|k| k.kid.as_deref()).collect();
    // oct excluded (symmetric), enc excluded (use=enc); rs and es remain.
    assert_eq!(kids, vec!["rs", "es"]);
}

#[test]
fn select_by_kid_honors_kid_over_alg() {
    let set = multi_key_set();
    // kid wins even though it names the encryption key.
    let k = set.select(Some("enc"), Some("RS256")).unwrap();
    assert_eq!(k.kid.as_deref(), Some("enc"));
    // Missing kid -> None.
    assert!(set.select(Some("absent"), None).is_none());
}

#[test]
fn select_by_alg_picks_matching_signing_key() {
    let set = multi_key_set();
    let k = set.select(None, Some("ES256")).unwrap();
    assert_eq!(k.kid.as_deref(), Some("es"));
    let k = set.select(None, Some("RS256")).unwrap();
    assert_eq!(k.kid.as_deref(), Some("rs"));
    // No alg -> first signing key (rs comes before es).
    let k = set.select(None, None).unwrap();
    assert_eq!(k.kid.as_deref(), Some("rs"));
    // alg with no matching signing key -> None.
    assert!(set.select(None, Some("PS512")).is_none());
}

#[test]
fn select_matches_key_without_alg_for_any_requested_alg() {
    let set = JsonWebKeySet {
        keys: vec![JsonWebKey {
            kty: KeyType::Rsa,
            key_use: Some(PublicKeyUse::Signature),
            kid: Some("no-alg".into()),
            alg: None,
            ..Default::default()
        }],
    };
    let k = set.select(None, Some("RS256")).unwrap();
    assert_eq!(k.kid.as_deref(), Some("no-alg"));
}

#[test]
fn empty_jwks_round_trips() {
    let set: JsonWebKeySet = serde_json::from_str(r#"{"keys":[]}"#).unwrap();
    assert!(set.keys.is_empty());
    let json = serde_json::to_value(&set).unwrap();
    assert_eq!(json["keys"], serde_json::json!([]));
}
