//! PKCE (Proof Key for Code Exchange) verification — RFC 7636.

use super::code::PkceMethod;

/// Verify a PKCE `code_verifier` against a stored `code_challenge` and `method`.
///
/// Call this at the token endpoint before exchanging an authorization code.
/// Returns `true` if the verifier is valid for the stored challenge.
///
/// ```
/// use klauthed_security::authz_code::{verify_pkce, PkceMethod};
///
/// let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
/// // In practice the client computes this at authorization time:
/// let challenge = s256(verifier);
/// assert!(verify_pkce(verifier, &challenge, PkceMethod::S256));
/// assert!(!verify_pkce("wrong-verifier", &challenge, PkceMethod::S256));
///
/// # fn s256(v: &str) -> String {
/// #     use ring::digest;
/// #     use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
/// #     URL_SAFE_NO_PAD.encode(digest::digest(&digest::SHA256, v.as_bytes()).as_ref())
/// # }
/// ```
#[must_use]
pub fn verify_pkce(verifier: &str, challenge: &str, method: PkceMethod) -> bool {
    let computed = match method {
        PkceMethod::Plain => verifier.to_owned(),
        PkceMethod::S256 => {
            use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
            use ring::digest;
            let hash = digest::digest(&digest::SHA256, verifier.as_bytes());
            URL_SAFE_NO_PAD.encode(hash.as_ref())
        }
    };
    // Constant-time comparison so verifier length leaks no information.
    crate::compare::constant_time_eq(computed.as_bytes(), challenge.as_bytes())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn s256(verifier: &str) -> String {
        use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
        use ring::digest;
        let hash = digest::digest(&digest::SHA256, verifier.as_bytes());
        URL_SAFE_NO_PAD.encode(hash.as_ref())
    }

    const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";

    #[test]
    fn plain_verifier_matches_challenge() {
        assert!(verify_pkce(VERIFIER, VERIFIER, PkceMethod::Plain));
        assert!(!verify_pkce("wrong", VERIFIER, PkceMethod::Plain));
    }

    #[test]
    fn s256_verifier_is_validated() {
        let challenge = s256(VERIFIER);
        assert!(verify_pkce(VERIFIER, &challenge, PkceMethod::S256));
        assert!(!verify_pkce("tampered", &challenge, PkceMethod::S256));
    }

    #[test]
    fn wrong_length_verifier_fails_safely() {
        let challenge = s256(VERIFIER);
        // A verifier that hashes to a different length is rejected without panic.
        assert!(!verify_pkce("x", &challenge, PkceMethod::S256));
    }
}
