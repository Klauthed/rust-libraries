//! Constant-time equality for secret material.
//!
//! Comparing secrets (MACs, tokens, password-derived bytes) with the usual `==`
//! can leak their contents through timing: it returns early at the first
//! differing byte. [`constant_time_eq`] always inspects every byte, so the time
//! it takes does not depend on *where* two equal-length inputs differ.

mod equal;

pub use equal::*;
