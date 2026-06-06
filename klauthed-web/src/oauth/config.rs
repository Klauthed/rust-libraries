//! `OAuthConfig` and its builder.

use std::sync::Arc;

use klauthed_core::time::{Clock, Duration, SystemClock};
use klauthed_security::{
    authz_code::AuthCodeStore, oauth2_client::ClientStore, refresh_token::RefreshTokenStore,
    JwtSigner,
};

// ── OAuthConfig ───────────────────────────────────────────────────────────────

/// Configuration shared by the authorization and token handlers.
///
/// Build with [`OAuthConfig::builder()`] and register as
/// `app_data(web::Data::new(config))`.
pub struct OAuthConfig {
    /// Client registration store.
    pub client_store: Arc<dyn ClientStore>,
    /// Short-lived authorization code store.
    pub code_store: Arc<dyn AuthCodeStore>,
    /// Refresh token store. When `Some`, the token endpoint issues and rotates
    /// refresh tokens; when `None`, only short-lived access tokens are issued.
    pub refresh_token_store: Option<Arc<dyn RefreshTokenStore>>,
    /// JWT signer used to mint access tokens.
    pub signer: JwtSigner,
    /// The `iss` claim placed in every access token.
    pub issuer: String,
    /// Access token lifetime (default: 1 hour).
    pub access_token_ttl: Duration,
    /// Authorization code lifetime (default: 5 minutes per RFC 6749).
    pub code_ttl: Duration,
    /// Refresh token lifetime (default: 30 days).
    pub refresh_token_ttl: Duration,
    /// Clock — injectable for deterministic tests.
    pub(crate) clock: Arc<dyn Clock>,
}

impl OAuthConfig {
    /// Start building an [`OAuthConfig`].
    pub fn builder() -> OAuthConfigBuilder {
        OAuthConfigBuilder::default()
    }
}

// ── OAuthConfigBuilder ────────────────────────────────────────────────────────

/// Builder for [`OAuthConfig`].
#[derive(Default)]
pub struct OAuthConfigBuilder {
    client_store: Option<Arc<dyn ClientStore>>,
    code_store: Option<Arc<dyn AuthCodeStore>>,
    refresh_token_store: Option<Arc<dyn RefreshTokenStore>>,
    signer: Option<JwtSigner>,
    issuer: Option<String>,
    access_token_ttl: Option<Duration>,
    code_ttl: Option<Duration>,
    refresh_token_ttl: Option<Duration>,
    clock: Option<Arc<dyn Clock>>,
}

impl OAuthConfigBuilder {
    /// Set the client registration store (required).
    #[must_use]
    pub fn client_store(mut self, store: Arc<dyn ClientStore>) -> Self {
        self.client_store = Some(store);
        self
    }

    /// Set the authorization code store (required).
    #[must_use]
    pub fn code_store(mut self, store: Arc<dyn AuthCodeStore>) -> Self {
        self.code_store = Some(store);
        self
    }

    /// Set the JWT signer used to mint access tokens (required).
    #[must_use]
    pub fn signer(mut self, signer: JwtSigner) -> Self {
        self.signer = Some(signer);
        self
    }

    /// Set the `iss` claim for issued access tokens (required).
    #[must_use]
    pub fn issuer(mut self, issuer: impl Into<String>) -> Self {
        self.issuer = Some(issuer.into());
        self
    }

    /// Set the access token lifetime (default: 1 hour).
    #[must_use]
    pub fn access_token_ttl(mut self, ttl: Duration) -> Self {
        self.access_token_ttl = Some(ttl);
        self
    }

    /// Enable refresh tokens with the given store.
    ///
    /// When set, the token endpoint issues a refresh token alongside the access
    /// token for `authorization_code` grants, and accepts `refresh_token` grants
    /// for token rotation.
    #[must_use]
    pub fn refresh_token_store(mut self, store: Arc<dyn RefreshTokenStore>) -> Self {
        self.refresh_token_store = Some(store);
        self
    }

    /// Set the authorization code lifetime (default: 5 minutes per RFC 6749).
    #[must_use]
    pub fn code_ttl(mut self, ttl: Duration) -> Self {
        self.code_ttl = Some(ttl);
        self
    }

    /// Set the refresh token lifetime (default: 30 days).
    #[must_use]
    pub fn refresh_token_ttl(mut self, ttl: Duration) -> Self {
        self.refresh_token_ttl = Some(ttl);
        self
    }

    /// Inject a clock (default: [`SystemClock`]).
    #[must_use]
    pub fn clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = Some(clock);
        self
    }

    /// Build the [`OAuthConfig`].
    ///
    /// # Panics
    /// Panics if `client_store`, `code_store`, `signer`, or `issuer` was not set.
    #[must_use]
    pub fn build(self) -> OAuthConfig {
        OAuthConfig {
            client_store: self.client_store.expect("OAuthConfig: client_store is required"),
            code_store: self.code_store.expect("OAuthConfig: code_store is required"),
            refresh_token_store: self.refresh_token_store,
            signer: self.signer.expect("OAuthConfig: signer is required"),
            issuer: self.issuer.expect("OAuthConfig: issuer is required"),
            access_token_ttl: self.access_token_ttl.unwrap_or_else(|| Duration::hours(1)),
            code_ttl: self.code_ttl.unwrap_or_else(|| Duration::minutes(5)),
            refresh_token_ttl: self
                .refresh_token_ttl
                .unwrap_or_else(|| Duration::days(30)),
            clock: self.clock.unwrap_or_else(|| Arc::new(SystemClock)),
        }
    }
}
