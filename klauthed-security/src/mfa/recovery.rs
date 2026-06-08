//! One-time MFA recovery codes (a.k.a. backup codes).
//!
//! A user is shown a set of single-use codes once, at enrollment, to fall back
//! on if they lose their authenticator. Only the SHA-256 hashes are persisted
//! (the codes are high-entropy, so a fast hash is appropriate — the same scheme
//! as [`apikey`](crate::apikey)); a leaked store therefore can't reveal a code.
//! Each code works exactly once.
//!
//! ```
//! use klauthed_security::mfa::RecoveryCodeSet;
//!
//! let generated = RecoveryCodeSet::generate(10).unwrap();
//! let shown_once = generated.codes.clone(); // render these to the user now
//! let mut stored = generated.stored;        // persist this (hashes only)
//!
//! // Later: the user submits one of their codes.
//! let code = &shown_once[0];
//! assert!(stored.verify_and_consume(code)); // accepted, now spent
//! assert!(!stored.verify_and_consume(code)); // one-time use: rejected
//! assert_eq!(stored.remaining(), 9);
//! ```

use ring::digest::{SHA256, digest};
use serde::{Deserialize, Serialize};

use crate::compare::constant_time_eq;
use crate::error::SecurityError;
use crate::token::random_bytes;

/// The default number of codes minted in a set.
pub const DEFAULT_RECOVERY_CODE_COUNT: usize = 10;

/// Bytes of entropy per code (64 bits → 16 hex characters).
const CODE_ENTROPY_BYTES: usize = 8;

/// Hex-encode the SHA-256 digest of `value`.
fn sha256_hex(value: &str) -> String {
    hex::encode(digest(&SHA256, value.as_bytes()).as_ref())
}

/// Normalize a user-entered code to its canonical hashing form: keep only the
/// alphanumeric characters, lowercased (so `ABCD-EF12` and `abcdef12` match).
fn normalize(code: &str) -> String {
    code.chars().filter(char::is_ascii_alphanumeric).map(|c| c.to_ascii_lowercase()).collect()
}

/// Format 16 hex characters as four dash-separated groups (`abcd-ef12-3456-7890`).
fn group(raw: &str) -> String {
    raw.as_bytes()
        .chunks(4)
        .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
        .collect::<Vec<_>>()
        .join("-")
}

/// A freshly generated set of recovery codes.
///
/// [`codes`](Self::codes) are the human-readable plaintext codes — display them
/// to the user **once**; they cannot be recovered from the stored hashes.
/// [`stored`](Self::stored) is the persistable [`RecoveryCodeSet`].
#[derive(Debug, Clone)]
pub struct GeneratedRecoveryCodes {
    /// The plaintext codes, formatted for display (e.g. `abcd-ef12-3456-7890`).
    pub codes: Vec<String>,
    /// The hashed set to persist and later verify against.
    pub stored: RecoveryCodeSet,
}

/// A persisted set of one-time recovery-code hashes.
///
/// Serializes to a list of hex SHA-256 hashes, so it can be stored as JSON
/// (e.g. a column on the user row). Verifying a code with
/// [`verify_and_consume`](Self::verify_and_consume) removes it from the set.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RecoveryCodeSet {
    hashes: Vec<String>,
}

impl RecoveryCodeSet {
    /// Generate `count` fresh recovery codes.
    ///
    /// Returns the plaintext codes (to show the user once) alongside the
    /// [`RecoveryCodeSet`] of their hashes (to persist).
    ///
    /// # Errors
    ///
    /// Returns [`SecurityError::Rng`] if the OS CSPRNG fails.
    pub fn generate(count: usize) -> Result<GeneratedRecoveryCodes, SecurityError> {
        let mut codes = Vec::with_capacity(count);
        let mut hashes = Vec::with_capacity(count);
        for _ in 0..count {
            let raw = hex::encode(random_bytes(CODE_ENTROPY_BYTES)?);
            hashes.push(sha256_hex(&raw));
            codes.push(group(&raw));
        }
        Ok(GeneratedRecoveryCodes { codes, stored: RecoveryCodeSet { hashes } })
    }

    /// Reconstruct a set from previously stored hashes (e.g. loaded from a DB).
    #[must_use]
    pub fn from_hashes(hashes: Vec<String>) -> Self {
        Self { hashes }
    }

    /// The stored hex hashes, for persistence.
    #[must_use]
    pub fn hashes(&self) -> &[String] {
        &self.hashes
    }

    /// How many unused codes remain.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.hashes.len()
    }

    /// Whether every code has been used (or none were generated).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.hashes.is_empty()
    }

    /// Verify `presented` against the unused codes; on a match, consume that
    /// code (so it cannot be reused) and return `true`.
    ///
    /// The input is normalized (case- and separator-insensitive) before hashing,
    /// and each stored hash is compared in constant time. Returns `false` for an
    /// unknown or already-used code; it never errors.
    #[must_use]
    pub fn verify_and_consume(&mut self, presented: &str) -> bool {
        let candidate = sha256_hex(&normalize(presented));
        let Some(index) = self
            .hashes
            .iter()
            .position(|stored| constant_time_eq(candidate.as_bytes(), stored.as_bytes()))
        else {
            return false;
        };
        self.hashes.swap_remove(index);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_produces_distinct_formatted_codes() {
        let set = RecoveryCodeSet::generate(10).unwrap();
        assert_eq!(set.codes.len(), 10);
        assert_eq!(set.stored.remaining(), 10);
        // Formatted as four 4-char groups.
        for code in &set.codes {
            assert_eq!(code.len(), 19, "{code}");
            assert_eq!(code.matches('-').count(), 3);
        }
        // All distinct.
        let mut sorted = set.codes.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 10);
    }

    #[test]
    fn verify_consumes_exactly_once() {
        let mut g = RecoveryCodeSet::generate(3).unwrap();
        let code = g.codes[1].clone();
        assert!(g.stored.verify_and_consume(&code));
        assert!(!g.stored.verify_and_consume(&code));
        assert_eq!(g.stored.remaining(), 2);
    }

    #[test]
    fn verify_is_insensitive_to_case_and_separators() {
        let mut g = RecoveryCodeSet::generate(1).unwrap();
        let canonical = g.codes[0].clone();
        let messy = format!("  {}  ", canonical.replace('-', "").to_uppercase());
        assert!(g.stored.verify_and_consume(&messy));
    }

    #[test]
    fn unknown_code_is_rejected() {
        let mut g = RecoveryCodeSet::generate(2).unwrap();
        assert!(!g.stored.verify_and_consume("0000-0000-0000-0000"));
        assert_eq!(g.stored.remaining(), 2);
    }

    #[test]
    fn serializes_as_a_hash_list() {
        let g = RecoveryCodeSet::generate(2).unwrap();
        let json = serde_json::to_string(&g.stored).unwrap();
        assert!(json.starts_with('['));
        let back: RecoveryCodeSet = serde_json::from_str(&json).unwrap();
        assert_eq!(back, g.stored);
    }

    #[test]
    fn empty_set_reports_empty() {
        let g = RecoveryCodeSet::generate(0).unwrap();
        assert!(g.stored.is_empty());
        assert!(g.codes.is_empty());
    }
}
