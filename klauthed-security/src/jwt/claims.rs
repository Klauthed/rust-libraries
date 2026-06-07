//! JWT [`Claims`] (registered + custom) and the fluent [`ClaimsBuilder`].

use serde::{Deserialize, Serialize};

use klauthed_core::time::{Clock, Duration};

use crate::error::SecurityError;
use crate::token::random_token;

/// JWT claims: the standard registered claims plus arbitrary custom claims.
///
/// All registered claims are optional so the struct can model both minted and
/// decoded tokens. Custom claims are flattened into the top-level JSON object,
/// so `claims.custom.insert("role", json!("admin"))` emits `"role":"admin"`
/// alongside `sub`, `exp`, etc.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claims {
    /// Subject — who the token is about (e.g. a user id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub: Option<String>,
    /// Issuer — who minted the token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iss: Option<String>,
    /// Audience — who the token is intended for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aud: Option<String>,
    /// Expiration time (seconds since the Unix epoch).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exp: Option<i64>,
    /// Issued-at time (seconds since the Unix epoch).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iat: Option<i64>,
    /// Not-before time (seconds since the Unix epoch).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nbf: Option<i64>,
    /// JWT id — a unique identifier for this token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jti: Option<String>,
    /// Any additional, application-specific claims.
    #[serde(flatten)]
    pub custom: serde_json::Map<String, serde_json::Value>,
}

impl Claims {
    /// Start building claims for `subject`, with `exp` = now + `ttl` derived from
    /// `clock`. `iat` and `nbf` are set to now. See [`ClaimsBuilder`].
    pub fn builder<C: Clock + ?Sized>(
        subject: impl Into<String>,
        clock: &C,
        ttl: Duration,
    ) -> ClaimsBuilder {
        let now = clock.now().unix_seconds();
        ClaimsBuilder {
            claims: Claims {
                sub: Some(subject.into()),
                iss: None,
                aud: None,
                exp: now.checked_add(ttl.whole_seconds()),
                iat: Some(now),
                nbf: Some(now),
                jti: None,
                custom: serde_json::Map::new(),
            },
        }
    }
}

/// A fluent builder for [`Claims`] (see [`Claims::builder`]).
#[derive(Debug, Clone)]
pub struct ClaimsBuilder {
    claims: Claims,
}

impl ClaimsBuilder {
    /// Set the issuer (`iss`).
    #[must_use]
    pub fn issuer(mut self, iss: impl Into<String>) -> Self {
        self.claims.iss = Some(iss.into());
        self
    }

    /// Set the audience (`aud`).
    #[must_use]
    pub fn audience(mut self, aud: impl Into<String>) -> Self {
        self.claims.aud = Some(aud.into());
        self
    }

    /// Set the JWT id (`jti`).
    #[must_use]
    pub fn jwt_id(mut self, jti: impl Into<String>) -> Self {
        self.claims.jti = Some(jti.into());
        self
    }

    /// Set a fresh random JWT id (`jti`), 16 bytes of entropy, url-safe base64.
    ///
    /// # Errors
    /// Returns [`SecurityError::Rng`] if the OS CSPRNG fails.
    pub fn random_jwt_id(mut self) -> Result<Self, SecurityError> {
        self.claims.jti = Some(random_token(16)?);
        Ok(self)
    }

    /// Insert a custom claim.
    #[must_use]
    pub fn claim(mut self, key: impl Into<String>, value: impl Into<serde_json::Value>) -> Self {
        self.claims.custom.insert(key.into(), value.into());
        self
    }

    /// Finish building.
    #[must_use]
    pub fn build(self) -> Claims {
        self.claims
    }
}
