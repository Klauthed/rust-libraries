//! The [`JsonWebKeySet`] document and key-lookup helpers.

use serde::{Deserialize, Serialize};

use crate::ProtocolError;

use super::{JsonWebKey, KeyType};

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
