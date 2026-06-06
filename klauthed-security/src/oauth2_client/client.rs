//! OAuth 2.0 client model: registration types and the `OAuth2Client` struct.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use klauthed_core::time::Timestamp;

// ── ClientType ────────────────────────────────────────────────────────────────

/// Whether a client can safely hold a client secret.
///
/// * [`Confidential`](ClientType::Confidential) — server-side apps that keep
///   credentials out of reach of end users.
/// * [`Public`](ClientType::Public) — clients that cannot guarantee secret
///   confidentiality (SPAs, native / mobile apps).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientType {
    /// Can securely store a client secret (server-side apps, services).
    Confidential,
    /// Cannot guarantee secret confidentiality (SPAs, native apps).
    Public,
}

// ── ClientGrantType ───────────────────────────────────────────────────────────

/// OAuth 2.0 grant types a client may be authorized to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ClientGrantType {
    /// RFC 6749 §4.1 — authorization code flow.
    AuthorizationCode,
    /// RFC 6749 §6 — refresh token exchange.
    RefreshToken,
    /// RFC 6749 §4.4 — machine-to-machine, no user.
    ClientCredentials,
}

// ── TokenEndpointAuthMethod ───────────────────────────────────────────────────

/// How a client authenticates at the token endpoint (RFC 7591 §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenEndpointAuthMethod {
    /// `client_secret_basic` — credentials in the HTTP `Authorization` header.
    ClientSecretBasic,
    /// `client_secret_post` — credentials in the URL-encoded request body.
    ClientSecretPost,
    /// `none` — no client authentication (public clients, PKCE-only flows).
    None,
}

// ── OAuth2Client ──────────────────────────────────────────────────────────────

/// A registered OAuth 2.0 / OIDC client (the server-side record).
///
/// The protocol wire types (`AuthorizationRequest`, `TokenRequest`, …) live in
/// `klauthed-protocol`; this is what the IDP stores and enforces.
///
/// # Security note
///
/// `client_secret_hash` stores an Argon2 PHC string (never the raw secret).
/// Use [`hash_password`](crate::hash_password) at registration time and
/// [`verify_password`](crate::verify_password) at the token endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuth2Client {
    /// The stable, server-issued client identifier (e.g. `"s6BhdRkqt3"`).
    pub client_id: String,

    /// Whether this client can hold a secret.
    pub client_type: ClientType,

    /// Argon2 PHC hash of the client secret. `None` for public clients.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret_hash: Option<String>,

    /// Registered redirect URIs — exact-match only, no pattern matching.
    pub redirect_uris: Vec<String>,

    /// The grant types this client is permitted to use.
    pub allowed_grant_types: BTreeSet<ClientGrantType>,

    /// The OAuth 2.0 scopes this client may request.
    pub allowed_scopes: BTreeSet<String>,

    /// How the client authenticates at the token endpoint.
    pub token_endpoint_auth_method: TokenEndpointAuthMethod,

    /// Human-readable name for admin UIs. Not used in protocol flows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_name: Option<String>,

    /// When this client was registered.
    pub created_at: Timestamp,
}

impl OAuth2Client {
    /// Return `true` if `redirect_uri` is in the registered list.
    ///
    /// Comparison is exact (byte-for-byte) per RFC 6749 §3.1.2.
    #[must_use]
    pub fn allows_redirect_uri(&self, redirect_uri: &str) -> bool {
        self.redirect_uris.iter().any(|u| u == redirect_uri)
    }

    /// Return `true` if this client is allowed to use `grant_type`.
    #[must_use]
    pub fn allows_grant(&self, grant_type: ClientGrantType) -> bool {
        self.allowed_grant_types.contains(&grant_type)
    }

    /// Return `true` if every scope in `requested` is within [`allowed_scopes`].
    ///
    /// [`allowed_scopes`]: OAuth2Client::allowed_scopes
    #[must_use]
    pub fn allows_scopes<'a>(&self, requested: impl IntoIterator<Item = &'a str>) -> bool {
        requested.into_iter().all(|s| self.allowed_scopes.contains(s))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn test_client() -> OAuth2Client {
        OAuth2Client {
            client_id: "c".into(),
            client_type: ClientType::Confidential,
            client_secret_hash: Some("$argon2id$...".into()),
            redirect_uris: vec![
                "https://app.example.com/cb".into(),
                "https://app.example.com/cb2".into(),
            ],
            allowed_grant_types: [
                ClientGrantType::AuthorizationCode,
                ClientGrantType::RefreshToken,
            ]
            .into_iter()
            .collect(),
            allowed_scopes: ["openid", "email", "profile"].iter().map(|s| s.to_string()).collect(),
            token_endpoint_auth_method: TokenEndpointAuthMethod::ClientSecretBasic,
            client_name: None,
            created_at: Timestamp::now(),
        }
    }

    #[test]
    fn allows_redirect_uri_exact_match() {
        let c = test_client();
        assert!(c.allows_redirect_uri("https://app.example.com/cb"));
        assert!(!c.allows_redirect_uri("https://app.example.com/cb3"));
        assert!(!c.allows_redirect_uri("https://APP.EXAMPLE.COM/cb")); // exact
    }

    #[test]
    fn allows_grant_checks_set() {
        let c = test_client();
        assert!(c.allows_grant(ClientGrantType::AuthorizationCode));
        assert!(c.allows_grant(ClientGrantType::RefreshToken));
        assert!(!c.allows_grant(ClientGrantType::ClientCredentials));
    }

    #[test]
    fn allows_scopes_checks_subset() {
        let c = test_client();
        assert!(c.allows_scopes(["openid", "email"]));
        assert!(c.allows_scopes(["openid"]));
        assert!(!c.allows_scopes(["openid", "admin"]));
        assert!(c.allows_scopes(std::iter::empty()));
    }
}
