//! Tests for the OIDC data types and ID token claim validation.

use klauthed_protocol::ProtocolError;
use klauthed_protocol::oidc::*;

fn base_claims() -> IdTokenClaims {
    IdTokenClaims {
        iss: "https://issuer.example.com".into(),
        sub: "248289761001".into(),
        aud: Audience::One("s6BhdRkqt3".into()),
        exp: 2_000_000_000,
        iat: 1_000_000_000,
        ..Default::default()
    }
}

#[test]
fn validate_id_token_happy_path() {
    let claims = base_claims();
    let opts = IdTokenValidation::new("https://issuer.example.com", "s6BhdRkqt3", 1_500_000_000)
        .with_leeway(60);
    assert!(validate_id_token(&claims, &opts).is_ok());
}

#[test]
fn validate_id_token_happy_path_with_nonce_and_array_aud() {
    let mut claims = base_claims();
    claims.aud = Audience::Many(vec!["other".into(), "s6BhdRkqt3".into()]);
    claims.nonce = Some("n-0S6_WzA2Mj".into());
    let opts = IdTokenValidation::new("https://issuer.example.com", "s6BhdRkqt3", 1_500_000_000)
        .with_nonce("n-0S6_WzA2Mj");
    assert!(validate_id_token(&claims, &opts).is_ok());
}

#[test]
fn validate_id_token_rejects_expired() {
    let claims = base_claims();
    // now is past exp.
    let opts = IdTokenValidation::new("https://issuer.example.com", "s6BhdRkqt3", 2_000_000_001);
    let err = validate_id_token(&claims, &opts).unwrap_err();
    assert!(matches!(err, ProtocolError::IdTokenExpired { .. }));
}

#[test]
fn validate_id_token_expiry_respects_leeway() {
    let claims = base_claims();
    // now == exp would fail without leeway (exp must be strictly after).
    let opts = IdTokenValidation::new("https://issuer.example.com", "s6BhdRkqt3", 2_000_000_030)
        .with_leeway(60);
    assert!(validate_id_token(&claims, &opts).is_ok());
}

#[test]
fn validate_id_token_rejects_wrong_issuer() {
    let claims = base_claims();
    let opts = IdTokenValidation::new("https://evil.example.com", "s6BhdRkqt3", 1_500_000_000);
    let err = validate_id_token(&claims, &opts).unwrap_err();
    match err {
        ProtocolError::IssuerMismatch { expected, actual } => {
            assert_eq!(expected, "https://evil.example.com");
            assert_eq!(actual, "https://issuer.example.com");
        }
        other => panic!("expected IssuerMismatch, got {other:?}"),
    }
}

#[test]
fn validate_id_token_rejects_audience_not_containing_client() {
    let claims = base_claims();
    let opts =
        IdTokenValidation::new("https://issuer.example.com", "different-client", 1_500_000_000);
    let err = validate_id_token(&claims, &opts).unwrap_err();
    assert!(matches!(err, ProtocolError::AudienceMismatch { .. }));
}

#[test]
fn validate_id_token_rejects_nonce_mismatch() {
    let mut claims = base_claims();
    claims.nonce = Some("actual".into());
    let opts = IdTokenValidation::new("https://issuer.example.com", "s6BhdRkqt3", 1_500_000_000)
        .with_nonce("expected");
    let err = validate_id_token(&claims, &opts).unwrap_err();
    assert!(matches!(err, ProtocolError::NonceMismatch));
}

#[test]
fn validate_id_token_rejects_missing_nonce_when_required() {
    let claims = base_claims();
    let opts = IdTokenValidation::new("https://issuer.example.com", "s6BhdRkqt3", 1_500_000_000)
        .with_nonce("expected");
    let err = validate_id_token(&claims, &opts).unwrap_err();
    assert!(matches!(err, ProtocolError::NonceMismatch));
}

#[test]
fn validate_id_token_rejects_future_iat() {
    let mut claims = base_claims();
    claims.iat = 1_600_000_000;
    let opts = IdTokenValidation::new("https://issuer.example.com", "s6BhdRkqt3", 1_500_000_000);
    let err = validate_id_token(&claims, &opts).unwrap_err();
    assert!(matches!(err, ProtocolError::IdTokenNotYetValid { .. }));
}

#[test]
fn provider_metadata_uses_exact_spec_keys() {
    let meta = ProviderMetadata {
        issuer: "https://issuer.example.com".into(),
        authorization_endpoint: Some("https://issuer.example.com/authorize".into()),
        token_endpoint: Some("https://issuer.example.com/token".into()),
        userinfo_endpoint: Some("https://issuer.example.com/userinfo".into()),
        jwks_uri: Some("https://issuer.example.com/jwks".into()),
        response_types_supported: vec![ResponseType::Code, ResponseType::CodeIdToken],
        grant_types_supported: vec![GrantType::AuthorizationCode, GrantType::RefreshToken],
        subject_types_supported: vec![SubjectType::Public],
        id_token_signing_alg_values_supported: vec!["RS256".into()],
        scopes_supported: vec![
            Scope::Known(KnownScope::OpenId),
            Scope::Known(KnownScope::Email),
            Scope::Other("custom".into()),
        ],
        claims_supported: vec!["sub".into(), "email".into()],
        ..Default::default()
    };

    let json = serde_json::to_value(&meta).unwrap();
    // Exact spec keys.
    assert!(json.get("issuer").is_some());
    assert!(json.get("response_types_supported").is_some());
    assert!(json.get("id_token_signing_alg_values_supported").is_some());
    assert!(json.get("subject_types_supported").is_some());
    // Enum string reps.
    assert_eq!(json["response_types_supported"][0], "code");
    assert_eq!(json["response_types_supported"][1], "code id_token");
    assert_eq!(json["grant_types_supported"][0], "authorization_code");
    assert_eq!(json["subject_types_supported"][0], "public");
    assert_eq!(json["scopes_supported"][0], "openid");
    assert_eq!(json["scopes_supported"][2], "custom");
    // Unset optional fields are omitted entirely.
    assert!(json.get("registration_endpoint").is_none());
    assert!(json.get("end_session_endpoint").is_none());
}

#[test]
fn provider_metadata_round_trips() {
    let json = r#"{
            "issuer": "https://issuer.example.com",
            "authorization_endpoint": "https://issuer.example.com/authorize",
            "token_endpoint": "https://issuer.example.com/token",
            "jwks_uri": "https://issuer.example.com/jwks",
            "response_types_supported": ["code"],
            "subject_types_supported": ["public", "pairwise"],
            "id_token_signing_alg_values_supported": ["RS256", "ES256"],
            "scopes_supported": ["openid", "profile", "email"],
            "custom_extension": {"vendor": "klauthed"}
        }"#;
    let meta: ProviderMetadata = serde_json::from_str(json).unwrap();
    assert_eq!(meta.issuer, "https://issuer.example.com");
    assert_eq!(meta.subject_types_supported.len(), 2);
    assert_eq!(meta.subject_types_supported[1], SubjectType::Pairwise);
    // Unmodeled members are preserved.
    assert!(meta.additional.contains_key("custom_extension"));

    let reser = serde_json::to_value(&meta).unwrap();
    assert_eq!(reser["custom_extension"]["vendor"], "klauthed");
}

#[test]
fn standard_claims_field_names() {
    let claims = StandardClaims {
        sub: Some("248289761001".into()),
        name: Some("Jane Doe".into()),
        given_name: Some("Jane".into()),
        family_name: Some("Doe".into()),
        preferred_username: Some("j.doe".into()),
        email: Some("janedoe@example.com".into()),
        email_verified: Some(true),
        locale: Some("en-US".into()),
        updated_at: Some(1_700_000_000),
        ..Default::default()
    };
    let json = serde_json::to_value(&claims).unwrap();
    assert_eq!(json["given_name"], "Jane");
    assert_eq!(json["family_name"], "Doe");
    assert_eq!(json["preferred_username"], "j.doe");
    assert_eq!(json["email_verified"], true);
    assert_eq!(json["updated_at"], 1_700_000_000);
    // gender unset -> omitted.
    assert!(json.get("gender").is_none());
}

#[test]
fn id_token_flattens_standard_claims_and_extras() {
    let json = r#"{
            "iss": "https://issuer.example.com",
            "sub": "248289761001",
            "aud": "s6BhdRkqt3",
            "exp": 1311281970,
            "iat": 1311280970,
            "auth_time": 1311280969,
            "nonce": "n-0S6_WzA2Mj",
            "acr": "urn:mace:incommon:iap:silver",
            "amr": ["pwd", "otp"],
            "azp": "s6BhdRkqt3",
            "email": "janedoe@example.com",
            "email_verified": true,
            "name": "Jane Doe",
            "groups": ["admins"]
        }"#;
    let claims: IdTokenClaims = serde_json::from_str(json).unwrap();
    assert_eq!(claims.iss, "https://issuer.example.com");
    assert_eq!(claims.aud, Audience::One("s6BhdRkqt3".into()));
    assert!(claims.aud.contains("s6BhdRkqt3"));
    assert_eq!(claims.amr, vec!["pwd", "otp"]);
    // Flattened standard claims.
    assert_eq!(claims.standard.email.as_deref(), Some("janedoe@example.com"));
    assert_eq!(claims.standard.email_verified, Some(true));
    assert_eq!(claims.standard.name.as_deref(), Some("Jane Doe"));
    // Flattened extras.
    assert_eq!(claims.additional["groups"][0], "admins");

    // Round-trip preserves the flat shape (no nested "standard" object).
    let reser = serde_json::to_value(&claims).unwrap();
    assert_eq!(reser["email"], "janedoe@example.com");
    assert_eq!(reser["name"], "Jane Doe");
    assert_eq!(reser["groups"][0], "admins");
    assert!(reser.get("standard").is_none());
    assert!(reser.get("additional").is_none());
}

#[test]
fn audience_supports_array_form() {
    let claims: IdTokenClaims =
        serde_json::from_str(r#"{"iss":"i","sub":"s","aud":["a","b"],"exp":1,"iat":0}"#).unwrap();
    assert_eq!(claims.aud, Audience::Many(vec!["a".into(), "b".into()]));
    let reser = serde_json::to_value(&claims).unwrap();
    assert_eq!(reser["aud"], serde_json::json!(["a", "b"]));
}
