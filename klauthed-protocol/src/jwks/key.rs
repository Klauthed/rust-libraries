//! Individual JSON Web Key types ([`JsonWebKey`], [`KeyType`], [`PublicKeyUse`]).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

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
