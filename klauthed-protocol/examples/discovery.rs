//! Build an OIDC provider-discovery document and print it as JSON.
//!
//! Run with: `cargo run -p klauthed-protocol --example discovery`

use klauthed_protocol::oidc::{ProviderMetadata, ResponseType, SubjectType};

fn main() {
    let meta = ProviderMetadata {
        issuer: "https://auth.example.com".into(),
        authorization_endpoint: Some("https://auth.example.com/oauth/authorize".into()),
        token_endpoint: Some("https://auth.example.com/oauth/token".into()),
        jwks_uri: Some("https://auth.example.com/oauth/jwks".into()),
        response_types_supported: vec![ResponseType::Code],
        subject_types_supported: vec![SubjectType::Public],
        id_token_signing_alg_values_supported: vec!["RS256".into()],
        ..Default::default()
    };

    let json = serde_json::to_string_pretty(&meta).expect("serialize");
    println!("{json}");
}
