//! OIDC JWKS endpoint — publishes the authorization server's public keys.
//!
//! This is pure protocol plumbing: the *key material* is app-supplied. Build a
//! [`JsonWebKeySet`] from your public keys (for an RS256 deployment), register
//! it as app data, and mount [`configure`]. Clients fetch it via the
//! `jwks_uri` advertised in discovery to verify token signatures.
//!
//! ```no_run
//! use actix_web::{web, App};
//! use klauthed_protocol::jwks::JsonWebKeySet;
//! use klauthed_web::oauth::jwks::configure as configure_jwks;
//!
//! # let key_set: JsonWebKeySet = todo!();
//! let _app = App::new()
//!     .app_data(web::Data::new(key_set))
//!     .configure(configure_jwks);
//! ```

use actix_web::{web, HttpResponse};
use klauthed_protocol::jwks::JsonWebKeySet;

/// `GET /oauth/jwks`
///
/// Returns the [`JsonWebKeySet`] registered as app data (`{"keys": [...]}`).
/// If no key set is registered the response is `404` — an HS256-only
/// deployment has no public keys to publish.
async fn jwks_handler(keys: Option<web::Data<JsonWebKeySet>>) -> HttpResponse {
    match keys {
        Some(k) => HttpResponse::Ok()
            .content_type("application/json")
            .json(k.as_ref()),
        None => HttpResponse::NotFound().finish(),
    }
}

/// Mount the JWKS route on an app.
///
/// Route added: `GET /oauth/jwks`. Register a [`JsonWebKeySet`] via
/// `app_data(web::Data::new(key_set))`; without one the route returns `404`.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/oauth/jwks", web::get().to(jwks_handler));
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::http::StatusCode;
    use actix_web::test as http_test;
    use actix_web::{web, App};
    use klauthed_protocol::jwks::{JsonWebKey, JsonWebKeySet, KeyType};

    fn key_set() -> JsonWebKeySet {
        JsonWebKeySet {
            keys: vec![JsonWebKey {
                kid: Some("key-1".into()),
                kty: KeyType::Rsa,
                ..Default::default()
            }],
        }
    }

    #[actix_web::test]
    async fn serves_registered_key_set() {
        let app = http_test::init_service(
            App::new()
                .app_data(web::Data::new(key_set()))
                .configure(configure),
        )
        .await;

        let req = http_test::TestRequest::get().uri("/oauth/jwks").to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body = http_test::read_body(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["keys"][0]["kid"], "key-1");
    }

    #[actix_web::test]
    async fn returns_404_without_registered_keys() {
        let app = http_test::init_service(App::new().configure(configure)).await;
        let req = http_test::TestRequest::get().uri("/oauth/jwks").to_request();
        let resp = http_test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
