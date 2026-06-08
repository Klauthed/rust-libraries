use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
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
    SystemRandom::new().fill(&mut buf).map_err(|_| SecurityError::Rng)?;
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
