//! JSON Web Key (JWK) and JSON Web Key Set (JWKS) data types (RFC 7517).
//!
//! Spec-accurate serde models for the documents an OIDC/OAuth 2.0 authorization
//! server publishes at its `jwks_uri`. These are pure *data* types plus
//! in-memory lookup: one [`JsonWebKey`] struct round-trips any single JWK
//! (RSA, EC, `oct`, OKP) because the key-type-specific material is modeled as
//! optional fields, and a [`JsonWebKeySet`] is a list of them with helpers for
//! the "pick the key for this token" case.
//!
//! Field names match the JSON wire format exactly. The one collision with a
//! Rust keyword is the `use` parameter, exposed here as
//! [`JsonWebKey::key_use`] (`#[serde(rename = "use")]`).
//!
//! # Out of scope
//!
//! This crate does **no** cryptography. Converting a [`JsonWebKey`] into a
//! concrete public/secret verification key and checking a JWT signature against
//! it is the job of `klauthed-security`, which consumes a key selected here
//! (e.g. via [`JsonWebKeySet::select`]). Nothing in this module fetches JWKS
//! over HTTP, decodes JWTs, or validates signatures.
//!
//! References:
//! * RFC 7517 (JSON Web Key)
//! * RFC 7518 (JSON Web Algorithms — `kty`, `crv`, parameter names)
//!
//! ```
//! use klauthed_protocol::jwks::{JsonWebKeySet, KeyType};
//!
//! let raw = r#"{"keys":[
//!   {"kty":"RSA","use":"sig","kid":"abc","alg":"RS256","n":"0vx7...","e":"AQAB"}
//! ]}"#;
//! let set: JsonWebKeySet = serde_json::from_str(raw).unwrap();
//! let key = set.find("abc").unwrap();
//! assert_eq!(key.kind(), Some(KeyType::Rsa));
//! assert_eq!(key.e.as_deref(), Some("AQAB"));
//! ```

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ProtocolError;

/// The `kty` (key type) parameter of a JWK (RFC 7518 section 6.1).
///
/// Serializes to the registered uppercase/lowercase forms exactly as they
/// appear on the wire (`"RSA"`, `"EC"`, `"oct"`, `"OKP"`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum KeyType {
    /// RSA key (`"RSA"`), parameters `n`/`e` (and private material).
    #[default]
    #[serde(rename = "RSA")]
    Rsa,
    /// Elliptic Curve key (`"EC"`), parameters `crv`/`x`/`y`.
    #[serde(rename = "EC")]
    Ec,
    /// Symmetric key (`"oct"`), parameter `k`.
    #[serde(rename = "oct")]
    Oct,
    /// Octet Key Pair for Edwards/Montgomery curves (`"OKP"`, RFC 8037).
    #[serde(rename = "OKP")]
    Okp,
}

/// The `use` (public key use) parameter of a JWK (RFC 7517 section 4.2).
///
/// Identifies whether a public key is meant for signatures or encryption.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum PublicKeyUse {
    /// `sig` — the key is used to verify signatures.
    #[serde(rename = "sig")]
    Signature,
    /// `enc` — the key is used to encrypt / for key agreement.
    #[serde(rename = "enc")]
    Encryption,
}

/// A single JSON Web Key (RFC 7517 section 4).
///
/// One struct models any JWK: the common parameters are named fields and the
/// key-type-specific material (`n`/`e` for RSA, `crv`/`x`/`y` for EC, `k` for
/// `oct`) is optional, present only for the relevant `kty`. Any parameter not
/// modeled here — including private-key material such as RSA `d`/`p`/`q` — is
/// captured in `additional` so the key round-trips losslessly.
///
/// All cryptographic material (`n`, `e`, `x`, `y`, `k`, …) is carried verbatim
/// as the base64url strings from the document; this crate does not decode it.
/// Turning these into a usable key lives in `klauthed-security`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct JsonWebKey {
    /// REQUIRED. The key type (`kty`).
    pub kty: KeyType,

    /// The intended public key use (`use`): signing or encryption.
    #[serde(default, rename = "use", skip_serializing_if = "Option::is_none")]
    pub key_use: Option<PublicKeyUse>,

    /// The key ID (`kid`) used to match a key against a JWS/JWE header.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kid: Option<String>,

    /// The algorithm (`alg`) the key is intended for (e.g. `RS256`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alg: Option<String>,

    /// The operations the key is permitted for (`key_ops`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub key_ops: Vec<String>,

    /// RSA modulus (`n`), base64url-encoded. Present for `kty = "RSA"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub n: Option<String>,

    /// RSA public exponent (`e`), base64url-encoded. Present for `kty = "RSA"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub e: Option<String>,

    /// EC/OKP curve name (`crv`), e.g. `P-256`, `Ed25519`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crv: Option<String>,

    /// EC/OKP `x` coordinate, base64url-encoded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<String>,

    /// EC `y` coordinate, base64url-encoded. Present for `kty = "EC"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<String>,

    /// Symmetric key value (`k`), base64url-encoded. Present for `kty = "oct"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub k: Option<String>,

    /// X.509 certificate chain (`x5c`); each entry is base64 (not base64url)
    /// DER per RFC 7517.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub x5c: Vec<String>,

    /// X.509 certificate SHA-1 thumbprint (`x5t`), base64url-encoded.
    #[serde(default, rename = "x5t", skip_serializing_if = "Option::is_none")]
    pub x5t: Option<String>,

    /// Any other JWK parameters not modeled above, preserved verbatim. This
    /// includes private-key material (`d`, `p`, `q`, …) and the
    /// `x5t#S256` thumbprint.
    #[serde(flatten)]
    pub additional: BTreeMap<String, Value>,
}

impl JsonWebKey {
    /// The key type of this JWK (`kty`).
    ///
    /// This is infallible — `kty` is a required, typed field — and returns
    /// `Some` for every well-formed key. It returns [`Option`] only to compose
    /// cleanly with the other accessors and to leave room for future,
    /// not-yet-modeled key types.
    pub fn kind(&self) -> Option<KeyType> {
        Some(self.kty)
    }

    /// Whether this key may be used to verify signatures.
    ///
    /// True when `use` is `sig` or is absent (an unspecified `use` is usable
    /// for any purpose per RFC 7517), and never when `use` is `enc`.
    pub fn is_signing_key(&self) -> bool {
        !matches!(self.key_use, Some(PublicKeyUse::Encryption))
    }
}

/// A JSON Web Key Set (RFC 7517 section 5): the document published at a
/// provider's `jwks_uri`.
///
/// The wire shape is `{"keys": [ ... ]}`; unknown top-level members are ignored
/// on deserialization. Use the helpers to resolve the key a JWS header points
/// at — then hand the chosen [`JsonWebKey`] to `klauthed-security` for the
/// actual signature check.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct JsonWebKeySet {
    /// The keys in the set, in document order.
    #[serde(default)]
    pub keys: Vec<JsonWebKey>,
}

impl JsonWebKeySet {
    /// Find the key with the given `kid`, if any.
    pub fn find(&self, kid: &str) -> Option<&JsonWebKey> {
        self.keys.iter().find(|k| k.kid.as_deref() == Some(kid))
    }

    /// Iterate over the keys usable for *signature verification*.
    ///
    /// A key qualifies when its `use` is `sig` or unset (see
    /// [`JsonWebKey::is_signing_key`]) and its `kty` is an asymmetric type that
    /// produces public verification keys (`RSA`, `EC`, `OKP`). Symmetric
    /// (`oct`) keys are excluded: a JWKS published for verification should not
    /// expose them, and they are not selected here.
    pub fn find_signing_keys(&self) -> impl Iterator<Item = &JsonWebKey> {
        self.keys.iter().filter(|k| k.is_signing_key() && !matches!(k.kty, KeyType::Oct))
    }

    /// Select the key to use for a token whose JWS header carries the given
    /// `kid` and/or `alg` — the common "which key verifies this token?" case.
    ///
    /// Resolution order:
    ///
    /// * If `kid` is `Some`, the key with that `kid` is returned (if present),
    ///   regardless of `alg`: a `kid` uniquely names a key, so honoring it is
    ///   what callers want even when the header `alg` and the JWK `alg` differ.
    /// * If `kid` is `None`, the first signing key (see
    ///   [`JsonWebKeySet::find_signing_keys`]) whose `alg` matches the requested
    ///   `alg` is returned; when `alg` is also `None`, the first signing key is
    ///   returned. A JWK with no `alg` of its own matches any requested `alg`.
    ///
    /// Returns `None` when nothing matches. This performs **no** cryptography;
    /// it only picks a candidate key for `klauthed-security` to verify against.
    pub fn select(&self, kid: Option<&str>, alg: Option<&str>) -> Option<&JsonWebKey> {
        if let Some(kid) = kid {
            return self.find(kid);
        }
        self.find_signing_keys().find(|k| match alg {
            Some(alg) => k.alg.as_deref().is_none_or(|a| a == alg),
            None => true,
        })
    }

    /// Find the key for a `kid`, returning [`ProtocolError::KeyNotFound`] when
    /// no key in the set carries it.
    ///
    /// A convenience over [`JsonWebKeySet::find`] for callers that want an error
    /// rather than an [`Option`] (e.g. resolving the `kid` from a JWS header).
    pub fn require(&self, kid: &str) -> Result<&JsonWebKey, ProtocolError> {
        self.find(kid).ok_or_else(|| ProtocolError::KeyNotFound { kid: kid.to_owned() })
    }
}

#[cfg(test)]
mod tests {
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
}
