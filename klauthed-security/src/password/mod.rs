//! Argon2id password hashing.
//!
//! Hashes are produced as self-describing [PHC strings] (algorithm, version,
//! parameters and a random salt all encoded inline), so verification needs only
//! the stored string and the candidate password.
//!
//! [PHC strings]: https://github.com/P-H-C/phc-string-format/blob/master/phc-sf-spec.md
//!
//! ```
//! use klauthed_security::password::{hash_password, verify_password};
//!
//! let phc = hash_password("correct horse battery staple").unwrap();
//! assert!(verify_password("correct horse battery staple", &phc).unwrap());
//! assert!(!verify_password("Tr0ub4dour&3", &phc).unwrap());
//! ```

mod argon2;

pub use argon2::*;
