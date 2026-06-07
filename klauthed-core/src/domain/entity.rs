//! Core building blocks: the [`Entity`] and [`ValueObject`] traits.

/// their ids are equal, regardless of their other fields.
pub trait Entity {
    /// The identity type, typically an [`Id<T>`](crate::id::Id).
    type Id;

    /// This entity's identity.
    fn id(&self) -> &Self::Id;
}

/// A marker for immutable values compared structurally (no identity).
///
/// Implement it on types like `Money`, `EmailAddress`, `DateRange` to document
/// and enforce value semantics.
pub trait ValueObject: Clone + PartialEq {}
