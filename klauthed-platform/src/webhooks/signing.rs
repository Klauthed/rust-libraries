//! HMAC-SHA256 webhook payload signing and verification ([`sign_payload`] /
//! [`verify_signature`]), Stripe-style `t=…,v1=…` headers.

use ring::hmac;

use crate::error::PlatformError;

/// The version tag emitted/required in the signature header (`v1=...`).
pub const SIGNATURE_VERSION: &str = "v1";

/// HMAC is taken over.
fn signing_input(timestamp_secs: i64, body: &[u8]) -> Vec<u8> {
    let mut input = Vec::with_capacity(body.len() + 16);
    input.extend_from_slice(timestamp_secs.to_string().as_bytes());
    input.push(b'.');
    input.extend_from_slice(body);
    input
}

/// Sign `body` with `secret` at `timestamp_secs` (Unix seconds) and return the
/// Stripe-style header value `t=<timestamp_secs>,v1=<hex>`.
///
/// The MAC is HMAC-SHA256 over `"{timestamp_secs}.{body}"`.
pub fn sign_payload(secret: &[u8], timestamp_secs: i64, body: &[u8]) -> String {
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret);
    let tag = hmac::sign(&key, &signing_input(timestamp_secs, body));
    format!("t={timestamp_secs},{SIGNATURE_VERSION}={}", hex::encode(tag.as_ref()))
}

/// Parse a `t=<secs>,v1=<hex>` header into its timestamp and the `v1` hex MAC.
fn parse_signature_header(header: &str) -> Option<(i64, String)> {
    let mut timestamp = None;
    let mut v1 = None;
    for part in header.split(',') {
        let (k, v) = part.split_once('=')?;
        match k.trim() {
            "t" => timestamp = v.trim().parse::<i64>().ok(),
            SIGNATURE_VERSION => v1 = Some(v.trim().to_owned()),
            _ => {}
        }
    }
    Some((timestamp?, v1?))
}

/// Verify a `t=<secs>,v1=<hex>` `header` against `body` using `secret`.
///
/// Recomputes the HMAC over `"{t}.{body}"` and compares it to the supplied `v1`
/// MAC in **constant time** (via [`ring::hmac::verify`]). Returns
/// [`PlatformError::WebhookSigning`] on a malformed header and
/// [`PlatformError::WebhookDelivery`] on a signature mismatch.
pub fn verify_signature(secret: &[u8], header: &str, body: &[u8]) -> Result<(), PlatformError> {
    let (timestamp, v1_hex) = parse_signature_header(header).ok_or_else(|| {
        PlatformError::WebhookSigning { message: "malformed signature header".to_owned() }
    })?;

    let provided = hex::decode(&v1_hex).map_err(|_| PlatformError::WebhookSigning {
        message: "signature is not valid hex".to_owned(),
    })?;

    let key = hmac::Key::new(hmac::HMAC_SHA256, secret);
    hmac::verify(&key, &signing_input(timestamp, body), &provided).map_err(|_| {
        PlatformError::WebhookDelivery { message: "webhook signature mismatch".to_owned() }
    })
}
