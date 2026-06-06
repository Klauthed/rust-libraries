//! Authorization and token endpoint handlers.
//!
//! * [`authorize`] — `GET /oauth/authorize`: validates the request, issues an
//!   [`AuthCode`](klauthed_security::AuthCode), and redirects to the client.
//! * [`token`] — `POST /oauth/token`: exchanges the code for a JWT access token.

use actix_web::{web, HttpResponse, ResponseError as _};
use klauthed_protocol::oauth2::{AuthorizationRequest, OAuth2ErrorCode, TokenRequest, TokenResponse, TokenType};
use klauthed_protocol::oidc::{GrantType, ResponseType};
use klauthed_security::{
    authz_code::{verify_pkce, AuthCodeBuilder, PkceMethod},
    oauth2_client::ClientGrantType,
    refresh_token::{ConsumeResult, RefreshTokenBuilder},
    Claims,
};

use crate::auth::AuthenticatedUser;
use crate::error::AppError;

use super::config::OAuthConfig;
use super::util::{error_redirect, redirect, redirect_url, token_error};

// ── /oauth/authorize ──────────────────────────────────────────────────────────

/// `GET /oauth/authorize`
///
/// Validates the authorization request parameters, issues an authorization
/// code for the authenticated user, and redirects to the client's
/// `redirect_uri` with `?code=…&state=…`.
///
/// Requires [`JwtAuth`](crate::auth::JwtAuth) middleware so that the
/// [`AuthenticatedUser`] extractor can identify the current user.
pub async fn authorize(
    query: web::Query<AuthorizationRequest>,
    user: AuthenticatedUser,
    config: web::Data<OAuthConfig>,
) -> HttpResponse {
    let req = query.into_inner();

    // ── 1. Validate the client ────────────────────────────────────────────────
    let client = match config.client_store.get(&req.client_id).await {
        Ok(Some(c)) => c,
        Ok(None) => {
            return HttpResponse::BadRequest().json(
                klauthed_protocol::oauth2::TokenErrorResponse::with_description(
                    OAuth2ErrorCode::InvalidClient,
                    "unknown client_id",
                ),
            );
        }
        Err(e) => {
            tracing::error!(error = %e, "client store lookup failed");
            return AppError::internal("client store unavailable").error_response();
        }
    };

    // ── 2. Resolve and validate redirect URI ──────────────────────────────────
    let redirect_uri = match req.redirect_uri.as_deref() {
        Some(uri) => {
            if !client.allows_redirect_uri(uri) {
                return HttpResponse::BadRequest().json(
                    klauthed_protocol::oauth2::TokenErrorResponse::with_description(
                        OAuth2ErrorCode::InvalidRequest,
                        "redirect_uri is not registered for this client",
                    ),
                );
            }
            uri.to_owned()
        }
        None => {
            if client.redirect_uris.len() == 1 {
                client.redirect_uris[0].clone()
            } else {
                return HttpResponse::BadRequest().json(
                    klauthed_protocol::oauth2::TokenErrorResponse::with_description(
                        OAuth2ErrorCode::InvalidRequest,
                        "redirect_uri is required when more than one is registered",
                    ),
                );
            }
        }
    };

    // ── 3. Validate response_type = code ──────────────────────────────────────
    if req.response_type != ResponseType::Code {
        return error_redirect(
            &redirect_uri,
            OAuth2ErrorCode::UnsupportedResponseType,
            "only response_type=code is supported",
            req.state.as_deref(),
        );
    }

    // ── 4. Validate grant type ────────────────────────────────────────────────
    if !client.allows_grant(ClientGrantType::AuthorizationCode) {
        return error_redirect(
            &redirect_uri,
            OAuth2ErrorCode::UnauthorizedClient,
            "client is not authorized for authorization_code grant",
            req.state.as_deref(),
        );
    }

    // ── 5. Validate scopes ────────────────────────────────────────────────────
    let requested_scopes: Vec<String> = req
        .scope
        .as_deref()
        .map(|s| s.split_whitespace().map(str::to_owned).collect())
        .unwrap_or_default();

    if !client.allows_scopes(requested_scopes.iter().map(String::as_str)) {
        return error_redirect(
            &redirect_uri,
            OAuth2ErrorCode::InvalidScope,
            "one or more requested scopes are not allowed for this client",
            req.state.as_deref(),
        );
    }

    // ── 6. Issue the authorization code ───────────────────────────────────────
    let subject = user.sub().unwrap_or("").to_owned();
    let mut builder = AuthCodeBuilder::new(&req.client_id, &subject)
        .redirect_uri(&redirect_uri)
        .scope(requested_scopes);

    if let Some(nonce) = req.nonce.as_deref() {
        builder = builder.nonce(nonce);
    }
    if let (Some(challenge), Some(method)) =
        (req.code_challenge.as_deref(), req.code_challenge_method)
    {
        let pkce_method = match method {
            klauthed_protocol::oauth2::CodeChallengeMethod::S256 => PkceMethod::S256,
            klauthed_protocol::oauth2::CodeChallengeMethod::Plain => PkceMethod::Plain,
            _ => {
                return error_redirect(
                    &redirect_uri,
                    OAuth2ErrorCode::InvalidRequest,
                    "unsupported code_challenge_method",
                    req.state.as_deref(),
                );
            }
        };
        builder = builder.pkce(challenge, pkce_method);
    }

    let code = match builder.build(&*config.clock, config.code_ttl) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "failed to generate authorization code");
            return error_redirect(
                &redirect_uri,
                OAuth2ErrorCode::ServerError,
                "could not generate authorization code",
                req.state.as_deref(),
            );
        }
    };

    let code_str = code.code.clone();
    if let Err(e) = config.code_store.store(code).await {
        tracing::error!(error = %e, "failed to store authorization code");
        return error_redirect(
            &redirect_uri,
            OAuth2ErrorCode::ServerError,
            "could not store authorization code",
            req.state.as_deref(),
        );
    }

    // ── 7. Redirect to the client ─────────────────────────────────────────────
    let mut params = vec![("code", code_str.as_str())];
    let state_owned;
    if let Some(s) = req.state.as_deref() {
        state_owned = s.to_owned();
        params.push(("state", &state_owned));
    }
    redirect(&redirect_url(&redirect_uri, &params))
}

// ── /oauth/token ──────────────────────────────────────────────────────────────

/// `POST /oauth/token` (`application/x-www-form-urlencoded`)
///
/// Dispatches on `grant_type`. Currently only `authorization_code` is
/// supported; other grant types return `unsupported_grant_type`.
pub async fn token(
    form: web::Form<TokenRequest>,
    config: web::Data<OAuthConfig>,
) -> HttpResponse {
    let req = form.into_inner();
    match req.grant_type {
        GrantType::AuthorizationCode => exchange_authorization_code(req, &config).await,
        GrantType::RefreshToken => exchange_refresh_token(req, &config).await,
        _ => token_error(
            OAuth2ErrorCode::UnsupportedGrantType,
            "only authorization_code and refresh_token grant types are supported",
        ),
    }
}

/// Exchange an authorization code for a JWT access token.
pub async fn exchange_authorization_code(
    req: TokenRequest,
    config: &OAuthConfig,
) -> HttpResponse {
    // ── 1. Require the code parameter ─────────────────────────────────────────
    let code_str = match req.code.as_deref() {
        Some(c) => c,
        None => return token_error(OAuth2ErrorCode::InvalidRequest, "code is required"),
    };

    // ── 2. Authenticate the client ────────────────────────────────────────────
    let client_id = match req.client_id.as_deref() {
        Some(id) => id,
        None => return token_error(OAuth2ErrorCode::InvalidRequest, "client_id is required"),
    };

    let client = match config.client_store.get(client_id).await {
        Ok(Some(c)) => c,
        Ok(None) => return token_error(OAuth2ErrorCode::InvalidClient, "unknown client_id"),
        Err(e) => {
            tracing::error!(error = %e, "client store lookup failed");
            return AppError::internal("client store unavailable").error_response();
        }
    };

    if let Some(secret_hash) = &client.client_secret_hash {
        let submitted = match req.client_secret.as_deref() {
            Some(s) => s,
            None => {
                return token_error(
                    OAuth2ErrorCode::InvalidClient,
                    "client_secret is required for confidential clients",
                );
            }
        };
        match klauthed_security::verify_password(submitted, secret_hash) {
            Ok(true) => {}
            Ok(false) => return token_error(OAuth2ErrorCode::InvalidClient, "invalid client_secret"),
            Err(e) => {
                tracing::error!(error = %e, "client secret verification failed");
                return AppError::internal("could not verify client credentials").error_response();
            }
        }
    }

    // ── 3. Consume the authorization code (single-use) ────────────────────────
    let auth_code = match config.code_store.consume(code_str).await {
        Ok(Some(c)) => c,
        Ok(None) => {
            return token_error(
                OAuth2ErrorCode::InvalidGrant,
                "authorization code is invalid, expired, or already used",
            );
        }
        Err(e) => {
            tracing::error!(error = %e, "code store consume failed");
            return AppError::internal("could not retrieve authorization code").error_response();
        }
    };

    // ── 4. Verify client_id binding ───────────────────────────────────────────
    if auth_code.client_id != client_id {
        return token_error(
            OAuth2ErrorCode::InvalidGrant,
            "authorization code was issued to a different client",
        );
    }

    // ── 5. Verify redirect_uri binding ────────────────────────────────────────
    if let Some(stored_uri) = &auth_code.redirect_uri {
        match req.redirect_uri.as_deref() {
            Some(req_uri) if req_uri == stored_uri => {}
            _ => {
                return token_error(
                    OAuth2ErrorCode::InvalidGrant,
                    "redirect_uri does not match the one used in the authorization request",
                );
            }
        }
    }

    // ── 6. Verify PKCE ────────────────────────────────────────────────────────
    if let (Some(challenge), Some(method)) = (&auth_code.pkce_challenge, auth_code.pkce_method) {
        let verifier = match req.code_verifier.as_deref() {
            Some(v) => v,
            None => {
                return token_error(
                    OAuth2ErrorCode::InvalidRequest,
                    "code_verifier is required for PKCE",
                );
            }
        };
        if !verify_pkce(verifier, challenge, method) {
            return token_error(
                OAuth2ErrorCode::InvalidGrant,
                "PKCE code_verifier does not match code_challenge",
            );
        }
    }

    // ── 7. Mint the access token ──────────────────────────────────────────────
    let scope_str = auth_code.scope.join(" ");
    let access_token = match mint_access_token(
        &auth_code.subject,
        scope_str.as_str(),
        client_id,
        config,
    ) {
        Ok(t) => t,
        Err(resp) => return resp,
    };

    // ── 8. Optionally issue a refresh token ───────────────────────────────────
    let refresh_token_value =
        issue_refresh_token(client_id, &auth_code.subject, &auth_code.scope, None, config)
            .await;

    HttpResponse::Ok().json(TokenResponse {
        access_token,
        token_type: TokenType::default(),
        expires_in: Some(config.access_token_ttl.whole_seconds()),
        scope: if scope_str.is_empty() { None } else { Some(scope_str) },
        refresh_token: refresh_token_value,
        id_token: None,
    })
}

// ── /oauth/token — refresh_token grant ───────────────────────────────────────

/// Exchange a refresh token for a new access token + rotated refresh token
/// (`grant_type=refresh_token`, RFC 6749 §6).
pub async fn exchange_refresh_token(req: TokenRequest, config: &OAuthConfig) -> HttpResponse {
    // ── 1. Require the refresh_token parameter ────────────────────────────────
    let token_str = match req.refresh_token.as_deref() {
        Some(t) => t,
        None => return token_error(OAuth2ErrorCode::InvalidRequest, "refresh_token is required"),
    };

    // ── 2. Require a configured refresh token store ───────────────────────────
    let rt_store = match &config.refresh_token_store {
        Some(s) => s,
        None => {
            return token_error(
                OAuth2ErrorCode::UnsupportedGrantType,
                "refresh_token grant is not configured",
            );
        }
    };

    // ── 3. Authenticate the client ────────────────────────────────────────────
    let client_id = match req.client_id.as_deref() {
        Some(id) => id,
        None => return token_error(OAuth2ErrorCode::InvalidRequest, "client_id is required"),
    };

    let client = match config.client_store.get(client_id).await {
        Ok(Some(c)) => c,
        Ok(None) => return token_error(OAuth2ErrorCode::InvalidClient, "unknown client_id"),
        Err(e) => {
            tracing::error!(error = %e, "client store lookup failed");
            return AppError::internal("client store unavailable").error_response();
        }
    };

    if let Some(secret_hash) = &client.client_secret_hash {
        let submitted = match req.client_secret.as_deref() {
            Some(s) => s,
            None => {
                return token_error(
                    OAuth2ErrorCode::InvalidClient,
                    "client_secret is required for confidential clients",
                );
            }
        };
        match klauthed_security::verify_password(submitted, secret_hash) {
            Ok(true) => {}
            Ok(false) => return token_error(OAuth2ErrorCode::InvalidClient, "invalid client_secret"),
            Err(e) => {
                tracing::error!(error = %e, "client secret verification failed");
                return AppError::internal("could not verify client credentials").error_response();
            }
        }
    }

    if !client.allows_grant(ClientGrantType::RefreshToken) {
        return token_error(
            OAuth2ErrorCode::UnauthorizedClient,
            "client is not authorized for refresh_token grant",
        );
    }

    // ── 4. Consume the refresh token (single-use with replay detection) ───────
    let rt = match rt_store.consume(token_str).await {
        Ok(ConsumeResult::Valid(rt)) => rt,
        Ok(ConsumeResult::Expired(_)) => {
            return token_error(OAuth2ErrorCode::InvalidGrant, "refresh token has expired");
        }
        Ok(ConsumeResult::Compromised { family_id }) => {
            tracing::warn!(
                family_id = %family_id,
                client_id = %client_id,
                "refresh token replay detected — family revoked"
            );
            return token_error(
                OAuth2ErrorCode::InvalidGrant,
                "refresh token reuse detected; all tokens in this session have been revoked",
            );
        }
        Ok(ConsumeResult::NotFound) => {
            return token_error(
                OAuth2ErrorCode::InvalidGrant,
                "refresh token is invalid or already used",
            );
        }
        Err(e) => {
            tracing::error!(error = %e, "refresh token store consume failed");
            return AppError::internal("could not retrieve refresh token").error_response();
        }
    };

    // ── 5. Verify client_id binding ───────────────────────────────────────────
    if rt.client_id != client_id {
        return token_error(
            OAuth2ErrorCode::InvalidGrant,
            "refresh token was issued to a different client",
        );
    }

    // ── 6. Determine scope (may be narrowed by the request) ───────────────────
    let scope = if let Some(requested_scope) = req.scope.as_deref() {
        let requested: Vec<String> =
            requested_scope.split_whitespace().map(str::to_owned).collect();
        // Each requested scope must be a subset of the original grant.
        let original: std::collections::HashSet<&str> = rt.scope.iter().map(String::as_str).collect();
        if !requested.iter().all(|s| original.contains(s.as_str())) {
            return token_error(
                OAuth2ErrorCode::InvalidScope,
                "requested scope exceeds the scope of the original refresh token",
            );
        }
        requested
    } else {
        rt.scope.clone()
    };

    // ── 7. Mint the new access token ──────────────────────────────────────────
    let scope_str = scope.join(" ");
    let access_token = match mint_access_token(&rt.subject, scope_str.as_str(), client_id, config) {
        Ok(t) => t,
        Err(resp) => return resp,
    };

    // ── 8. Issue a rotated refresh token (same family_id) ─────────────────────
    let new_refresh_token = issue_refresh_token(
        client_id,
        &rt.subject,
        &scope,
        Some(rt.family_id.as_str()),
        config,
    )
    .await;

    HttpResponse::Ok().json(TokenResponse {
        access_token,
        token_type: TokenType::default(),
        expires_in: Some(config.access_token_ttl.whole_seconds()),
        scope: if scope_str.is_empty() { None } else { Some(scope_str) },
        refresh_token: new_refresh_token,
        id_token: None,
    })
}

// ── Shared helpers ────────────────────────────────────────────────────────────

/// Mint a signed JWT access token, returning `Err(HttpResponse)` on failure.
fn mint_access_token(
    subject: &str,
    scope: &str,
    client_id: &str,
    config: &OAuthConfig,
) -> Result<String, HttpResponse> {
    let builder = Claims::builder(subject, &*config.clock, config.access_token_ttl)
        .issuer(config.issuer.as_str())
        .claim("scope", scope)
        .claim("client_id", client_id);

    let claims = match builder.random_jwt_id() {
        Ok(c) => c.build(),
        Err(e) => {
            tracing::error!(error = %e, "failed to generate jwt id");
            return Err(AppError::internal("could not generate access token").error_response());
        }
    };

    config.signer.encode(&claims).map_err(|e| {
        tracing::error!(error = %e, "jwt signing failed");
        AppError::internal("could not sign access token").error_response()
    })
}

/// Issue a refresh token if the store is configured.
///
/// `family_id`: `Some(id)` for rotation (keeps the same family), `None` for
/// initial issuance (mints a fresh family).
///
/// Returns `Some(bearer_value)` on success, `None` when no store is configured
/// or on non-fatal error (logged but not returned to the client).
async fn issue_refresh_token(
    client_id: &str,
    subject: &str,
    scope: &[String],
    family_id: Option<&str>,
    config: &OAuthConfig,
) -> Option<String> {
    let store = config.refresh_token_store.as_ref()?;

    let mut builder = RefreshTokenBuilder::new(client_id, subject).scope(scope.to_vec());
    if let Some(fid) = family_id {
        builder = builder.family_id(fid);
    }

    let rt = match builder.build(&*config.clock, config.refresh_token_ttl) {
        Ok(rt) => rt,
        Err(e) => {
            tracing::error!(error = %e, "failed to build refresh token");
            return None;
        }
    };

    let bearer = rt.token.clone();
    if let Err(e) = store.store(rt).await {
        tracing::error!(error = %e, "failed to store refresh token");
        return None;
    }

    Some(bearer)
}
