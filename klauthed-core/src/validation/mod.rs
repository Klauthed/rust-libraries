//! Lightweight, structured validation.
//!
//! A type implements [`Validate`] to check its own invariants, accumulating
//! every problem into [`ValidationErrors`] (rather than failing on the first).
//! Each [`ValidationError`] carries an optional `field`, a stable `code`, and a
//! human message — so the same data drives both API responses and logs.
//!
//! [`ValidationErrors`] is a [`DomainError`](klauthed_error::DomainError) (category `BadRequest`,
//! code `validation.failed`), so it flows through the shared error handling.
//!
//! ```
//! use klauthed_core::validation::{Validate, ValidationErrors};
//!
//! struct SignUp { email: String, age: u8 }
//!
//! impl Validate for SignUp {
//!     fn validate(&self) -> Result<(), ValidationErrors> {
//!         let mut errors = ValidationErrors::new();
//!         if !self.email.contains('@') {
//!             errors.add("email", "invalid_email", "must be a valid email address");
//!         }
//!         if self.age < 18 {
//!             errors.add("age", "too_small", "must be at least 18");
//!         }
//!         errors.into_result()
//!     }
//! }
//!
//! assert!(SignUp { email: "x".into(), age: 10 }.validate().is_err());
//! assert!(SignUp { email: "a@b.co".into(), age: 21 }.validate().is_ok());
//! ```

pub mod errors;
pub mod validate;

pub use errors::{ValidationError, ValidationErrors};
pub use validate::Validate;
