//! Constant-time equality for secret material.
//!
//! Comparing secrets (MACs, tokens, password-derived bytes) with the usual `==`
//! can leak their contents through timing: it returns early at the first
//! differing byte. [`constant_time_eq`] always inspects every byte, so the time
//! it takes does not depend on *where* two equal-length inputs differ.

use subtle::ConstantTimeEq;

/// Compare two byte slices in constant time (for equal-length inputs).
///
/// Length is checked up front (lengths are not themselves secret), then the
/// bytes are compared with no data-dependent branches.
///
/// ```
/// use klauthed_security::compare::constant_time_eq;
///
/// assert!(constant_time_eq(b"token-abc", b"token-abc"));
/// assert!(!constant_time_eq(b"token-abc", b"token-xyz"));
/// assert!(!constant_time_eq(b"short", b"longer"));
/// ```
#[must_use]
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.ct_eq(b).into()
}
