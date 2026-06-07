//! Public-API integration tests for the protocol wire types.

use klauthed_protocol::oidc::{
    Audience, IdTokenClaims, IdTokenValidation, ProviderMetadata, ResponseType, SubjectType,
    validate_id_token,
};

#[test]
fn provider_metadata_serializes_to_spec_field_names() {
    let meta = ProviderMetadata {
        issuer: "https://issuer.example.com".into(),
        authorization_endpoint: Some("https://issuer.example.com/authorize".into()),
        token_endpoint: Some("https://issuer.example.com/token".into()),
        response_types_supported: vec![ResponseType::Code],
        subject_types_supported: vec![SubjectType::Public],
        id_token_signing_alg_values_supported: vec!["RS256".into()],
        ..Default::default()
    };
    let json = serde_json::to_value(&meta).unwrap();
    assert_eq!(json["issuer"], "https://issuer.example.com");
    assert_eq!(json["response_types_supported"][0], "code");
    assert_eq!(json["subject_types_supported"][0], "public");
}

#[test]
fn id_token_claims_validate() {
    let claims = IdTokenClaims {
        iss: "https://issuer.example.com".into(),
        sub: "user-1".into(),
        aud: Audience::One("client-1".into()),
        exp: 2_000_000_000,
        iat: 1_000_000_000,
        ..Default::default()
    };

    // Valid within the window.
    let ok = IdTokenValidation::new("https://issuer.example.com", "client-1", 1_500_000_000);
    assert!(validate_id_token(&claims, &ok).is_ok());

    // Wrong audience is rejected.
    let bad_aud =
        IdTokenValidation::new("https://issuer.example.com", "other-client", 1_500_000_000);
    assert!(validate_id_token(&claims, &bad_aud).is_err());
}
