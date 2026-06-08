//! Cryptographically secure random tokens.
//!
//! For session ids, API keys, CSRF tokens, password-reset nonces and the like.
//! Bytes come from the OS CSPRNG (via `ring`); [`random_token`] renders them as
//! URL-safe, unpadded base64 so the result is safe in URLs, headers and cookies.

mod random;

pub use random::*;
