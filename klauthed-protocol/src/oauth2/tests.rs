//! Tests for the OAuth 2.0 message and parameter types.

use crate::oidc::{GrantType, ResponseType};

use super::*;

#[test]
fn authorization_request_uses_exact_spec_keys() {
    let req = AuthorizationRequest {
        response_type: ResponseType::Code,
        client_id: "s6BhdRkqt3".into(),
        redirect_uri: Some("https://rp.example.com/cb".into()),
        scope: Some("openid email".into()),
        state: Some("xyz".into()),
        nonce: Some("n-0S6_WzA2Mj".into()),
        code_challenge: Some("E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM".into()),
        code_challenge_method: Some(CodeChallengeMethod::S256),
        ..Default::default()
    };
    let json = serde_json::to_value(&req).unwrap();
    assert_eq!(json["response_type"], "code");
    assert_eq!(json["client_id"], "s6BhdRkqt3");
    assert_eq!(json["redirect_uri"], "https://rp.example.com/cb");
    assert_eq!(json["scope"], "openid email");
    assert_eq!(json["state"], "xyz");
    assert_eq!(json["code_challenge_method"], "S256");
    // Unset optionals omitted.
    assert!(json.get("max_age").is_none());
    assert!(json.get("login_hint").is_none());
}

#[test]
fn code_challenge_method_plain() {
    let json = serde_json::to_value(CodeChallengeMethod::Plain).unwrap();
    assert_eq!(json, "plain");
    let back: CodeChallengeMethod = serde_json::from_value(json).unwrap();
    assert_eq!(back, CodeChallengeMethod::Plain);
}

#[test]
fn token_request_uses_exact_spec_keys() {
    let req = TokenRequest {
        grant_type: GrantType::AuthorizationCode,
        code: Some("SplxlOBeZQQYbYS6WxSbIA".into()),
        redirect_uri: Some("https://rp.example.com/cb".into()),
        client_id: Some("s6BhdRkqt3".into()),
        code_verifier: Some("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk".into()),
        ..Default::default()
    };
    let json = serde_json::to_value(&req).unwrap();
    assert_eq!(json["grant_type"], "authorization_code");
    assert_eq!(json["code"], "SplxlOBeZQQYbYS6WxSbIA");
    assert_eq!(json["redirect_uri"], "https://rp.example.com/cb");
    assert_eq!(json["client_id"], "s6BhdRkqt3");
    assert_eq!(json["code_verifier"], "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk");
    // Unused fields omitted.
    assert!(json.get("refresh_token").is_none());
    assert!(json.get("client_secret").is_none());
}

#[test]
fn token_request_refresh_round_trips() {
    let json = r#"{"grant_type":"refresh_token","refresh_token":"tGzv3JOkF0XG5Qx2TlKWIA","scope":"openid"}"#;
    let req: TokenRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.grant_type, GrantType::RefreshToken);
    assert_eq!(req.refresh_token.as_deref(), Some("tGzv3JOkF0XG5Qx2TlKWIA"));
    assert_eq!(req.scope.as_deref(), Some("openid"));
    let reser = serde_json::to_value(&req).unwrap();
    assert_eq!(reser["grant_type"], "refresh_token");
    assert!(reser.get("code").is_none());
}

#[test]
fn token_response_uses_exact_spec_keys() {
    let resp = TokenResponse {
        access_token: "2YotnFZFEjr1zCsicMWpAA".into(),
        token_type: TokenType::default(),
        expires_in: Some(3600),
        refresh_token: Some("tGzv3JOkF0XG5Qx2TlKWIA".into()),
        id_token: Some("eyJ...".into()),
        scope: Some("openid email".into()),
    };
    let json = serde_json::to_value(&resp).unwrap();
    assert_eq!(json["access_token"], "2YotnFZFEjr1zCsicMWpAA");
    assert_eq!(json["token_type"], "Bearer");
    assert_eq!(json["expires_in"], 3600);
    assert_eq!(json["refresh_token"], "tGzv3JOkF0XG5Qx2TlKWIA");
    assert_eq!(json["id_token"], "eyJ...");
    assert_eq!(json["scope"], "openid email");
}

#[test]
fn token_response_round_trips_and_omits_absent() {
    let json = r#"{"access_token":"abc","token_type":"Bearer"}"#;
    let resp: TokenResponse = serde_json::from_str(json).unwrap();
    assert_eq!(resp.access_token, "abc");
    assert_eq!(resp.token_type, TokenType::Known(KnownTokenType::Bearer));
    assert!(resp.id_token.is_none());
    let reser = serde_json::to_value(&resp).unwrap();
    assert!(reser.get("expires_in").is_none());
    assert!(reser.get("id_token").is_none());
}

#[test]
fn token_type_accepts_lowercase_and_custom() {
    let lower: TokenType = serde_json::from_str("\"bearer\"").unwrap();
    assert_eq!(lower, TokenType::Known(KnownTokenType::Bearer));
    let custom: TokenType = serde_json::from_str("\"mac\"").unwrap();
    assert_eq!(custom, TokenType::Other("mac".into()));
}

#[test]
fn error_codes_serialize_to_spec_strings() {
    assert_eq!(serde_json::to_value(OAuth2ErrorCode::InvalidRequest).unwrap(), "invalid_request");
    assert_eq!(serde_json::to_value(OAuth2ErrorCode::InvalidClient).unwrap(), "invalid_client");
    assert_eq!(serde_json::to_value(OAuth2ErrorCode::InvalidGrant).unwrap(), "invalid_grant");
    assert_eq!(
        serde_json::to_value(OAuth2ErrorCode::UnauthorizedClient).unwrap(),
        "unauthorized_client"
    );
    assert_eq!(
        serde_json::to_value(OAuth2ErrorCode::UnsupportedGrantType).unwrap(),
        "unsupported_grant_type"
    );
    assert_eq!(serde_json::to_value(OAuth2ErrorCode::InvalidScope).unwrap(), "invalid_scope");
}

#[test]
fn token_error_response_shape() {
    let err = TokenErrorResponse::with_description(
        OAuth2ErrorCode::InvalidGrant,
        "authorization code expired",
    );
    let json = serde_json::to_value(&err).unwrap();
    assert_eq!(json["error"], "invalid_grant");
    assert_eq!(json["error_description"], "authorization code expired");
    assert!(json.get("error_uri").is_none());

    let parsed: TokenErrorResponse = serde_json::from_value(json).unwrap();
    assert_eq!(parsed.error, OAuth2ErrorCode::InvalidGrant);

    let bare = TokenErrorResponse::new(OAuth2ErrorCode::InvalidClient);
    let bare_json = serde_json::to_value(&bare).unwrap();
    assert_eq!(bare_json["error"], "invalid_client");
    assert!(bare_json.get("error_description").is_none());
}

#[test]
fn scope_helpers_round_trip() {
    assert_eq!(scope_to_string(["openid", "email", "profile"]), "openid email profile");
    assert_eq!(scope_to_string(Vec::<String>::new()), "");
    assert_eq!(scope_from_str("openid  email "), vec!["openid".to_string(), "email".to_string()]);
    assert_eq!(scope_from_str(""), Vec::<String>::new());
}
