//! Cryptographically secure random tokens.
//!
//! For session ids, API keys, CSRF tokens, password-reset nonces and the like.
//! Bytes come from the OS CSPRNG (via `ring`); [`random_token`] renders them as
//! URL-safe, unpadded base64 so the result is safe in URLs, headers and cookies.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use ring::rand::{SecureRandom, SystemRandom};

use crate::error::SecurityError;

/// Fill a fresh `n`-byte buffer from the OS CSPRNG.
///
/// ```
/// use klauthed_security::token::random_bytes;
///
/// let b = random_bytes(32).unwrap();
/// assert_eq!(b.len(), 32);
/// ```
pub fn random_bytes(n: usize) -> Result<Vec<u8>, SecurityError> {
    let mut buf = vec![0u8; n];
    SystemRandom::new()
        .fill(&mut buf)
        .map_err(|_| SecurityError::Rng)?;
    Ok(buf)
}

/// A URL-safe, unpadded base64 token carrying `byte_len` bytes of entropy.
///
/// The returned string is longer than `byte_len` (base64 expansion). For typical
/// uses, 32 bytes (256 bits) is a strong default.
///
/// ```
/// use klauthed_security::token::random_token;
///
/// let t = random_token(32).unwrap();
/// // URL-safe alphabet, no padding.
/// assert!(!t.contains('+') && !t.contains('/') && !t.contains('='));
/// assert_ne!(random_token(32).unwrap(), random_token(32).unwrap());
/// ```
pub fn random_token(byte_len: usize) -> Result<String, SecurityError> {
    let bytes = random_bytes(byte_len)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compare::constant_time_eq;

    #[test]
    fn random_bytes_have_requested_length() {
        assert_eq!(random_bytes(0).unwrap().len(), 0);
        assert_eq!(random_bytes(48).unwrap().len(), 48);
    }

    #[test]
    fn tokens_are_url_safe_and_unique() {
        let a = random_token(32).unwrap();
        let b = random_token(32).unwrap();
        assert_ne!(a, b);
        for t in [&a, &b] {
            assert!(t.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'));
        }
        assert!(!constant_time_eq(a.as_bytes(), b.as_bytes()));
    }

    #[test]
    fn decodes_back_to_requested_entropy() {
        let t = random_token(16).unwrap();
        let decoded = URL_SAFE_NO_PAD.decode(t).unwrap();
        assert_eq!(decoded.len(), 16);
    }
}
