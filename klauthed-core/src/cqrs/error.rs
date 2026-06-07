//! The [`CqrsError`] returned by the dispatch buses.

use std::any::type_name;

use klauthed_error::{DomainError, ErrorCategory, ErrorCode};

/// Errors raised by the dispatch buses.
#[derive(Debug)]
pub enum CqrsError {
    /// No handler was registered for the dispatched message type.
    NoHandler {
        /// The message type that had no handler.
        message_type: &'static str,
    },
    /// A handler returned an error.
    Handler(Box<dyn DomainError + Send + Sync>),
}

impl CqrsError {
    pub(crate) fn no_handler<M: 'static>() -> Self {
        CqrsError::NoHandler { message_type: type_name::<M>() }
    }

    /// Wrap a handler's [`DomainError`].
    pub fn handler<E: DomainError + Send + Sync + 'static>(error: E) -> Self {
        CqrsError::Handler(Box::new(error))
    }
}

impl std::fmt::Display for CqrsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CqrsError::NoHandler { message_type } => {
                write!(f, "no handler registered for '{message_type}'")
            }
            CqrsError::Handler(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for CqrsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CqrsError::NoHandler { .. } => None,
            CqrsError::Handler(error) => Some(&**error),
        }
    }
}

impl DomainError for CqrsError {
    fn category(&self) -> ErrorCategory {
        match self {
            // A missing handler is a wiring bug, not a caller error.
            CqrsError::NoHandler { .. } => ErrorCategory::Internal,
            CqrsError::Handler(error) => error.category(),
        }
    }

    fn code(&self) -> ErrorCode {
        match self {
            CqrsError::NoHandler { .. } => ErrorCode::new("cqrs.no_handler"),
            CqrsError::Handler(error) => error.code(),
        }
    }
}
