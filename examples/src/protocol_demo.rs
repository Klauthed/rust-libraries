//! `klauthed-protocol`: OIDC ID-token validation and OAuth2 wire types.

use klauthed_protocol::oauth2::messages::TokenResponse;
use klauthed_protocol::oauth2::params::{KnownTokenType, TokenType};
use klauthed_protocol::oauth2::scope::{scope_from_str, scope_to_string};
use klauthed_protocol::oidc::{Audience, IdTokenClaims, IdTokenValidation, validate_id_token};

pub fn run() {
    // OIDC: validate an ID token's registered claims against expectations.
    let claims = IdTokenClaims {
        iss: "https://issuer.example.com".into(),
        sub: "248289761001".into(),
        aud: Audience::One("client-abc".into()),
        exp: 2_000_000_000,
        iat: 1_000_000_000,
        ..Default::default()
    };
    let opts = IdTokenValidation::new("https://issuer.example.com", "client-abc", 1_500_000_000)
        .with_leeway(60);
    assert!(validate_id_token(&claims, &opts).is_ok());
    // A token meant for a different client is rejected.
    let wrong_aud =
        IdTokenValidation::new("https://issuer.example.com", "other-client", 1_500_000_000);
    assert!(validate_id_token(&claims, &wrong_aud).is_err());
    println!("  oidc: id_token validated; wrong-audience rejected");

    // OAuth2: a token response serializes to the spec wire shape.
    let resp = TokenResponse {
        access_token: "at-123".into(),
        token_type: TokenType::Known(KnownTokenType::Bearer),
        expires_in: Some(3600),
        refresh_token: Some("rt-456".into()),
        id_token: None,
        scope: Some("openid email".into()),
    };
    let json = serde_json::to_value(&resp).unwrap();
    assert_eq!(json["token_type"], "Bearer"); // exact RFC casing
    assert!(json.get("id_token").is_none()); // absent fields are skipped
    println!("  oauth2: token_type serializes as {}", json["token_type"]);

    // Scope helpers round-trip the space-delimited form.
    let s = scope_to_string(["openid", "email", "profile"]);
    assert_eq!(scope_from_str(&s), vec!["openid", "email", "profile"]);
    println!("  oauth2: scopes = {s:?}");
}
