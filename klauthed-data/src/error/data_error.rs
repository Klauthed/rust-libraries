use klauthed_core::config::{CacheBackend, DbSystem, MessagingBackend};
use klauthed_core::error::ConfigError;
use klauthed_error::{DomainError, ErrorCategory, ErrorCode};
use thiserror::Error;

/// Errors raised while turning configuration into live data connections.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DataError {
    /// A configuration error surfaced while building a resource. Its category
    /// and code delegate to the underlying [`ConfigError`].
    #[error("configuration error: {0}")]
    Config(#[from] ConfigError),

    /// The configured database system has no connector in this build.
    #[error("database system '{0:?}' is not supported by this connector")]
    UnsupportedSystem(DbSystem),

    /// The configured cache backend has no connector in this build.
    #[error("cache backend '{0:?}' is not supported by this connector")]
    UnsupportedCacheBackend(CacheBackend),

    /// The configured messaging backend has no connector in this build.
    #[error("messaging backend '{0:?}' is not supported by this connector")]
    UnsupportedMessagingBackend(MessagingBackend),

    /// No connection URL could be derived from the provided configuration.
    #[error("no connection URL could be derived for {0}")]
    MissingUrl(&'static str),

    /// A messaging client failed to set up or connect.
    #[error("messaging setup error: {0}")]
    Messaging(String),

    /// The transactional outbox encountered an error.
    #[error("transactional outbox error: {0}")]
    Outbox(String),

    /// The idempotency store encountered an error.
    #[error("idempotency store error: {0}")]
    Idempotency(String),

    /// A schema migration failed or is invalid (e.g. a duplicate version).
    #[error("migration error: {0}")]
    Migration(String),

    /// A named distributed lock is already held by another holder.
    #[error("lock '{0}' is already held")]
    LockHeld(String),

    /// The requested backend needs a cargo feature that is not enabled.
    #[error("this backend requires the '{0}' cargo feature to be enabled")]
    FeatureDisabled(&'static str),

    /// An underlying I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// A SQL driver error (`sql` feature).
    #[cfg(feature = "sql")]
    #[error("database error: {0}")]
    Sqlx(#[from] sqlx::Error),

    /// A Redis client error (`redis` feature).
    #[cfg(feature = "redis")]
    #[error("redis error: {0}")]
    Redis(#[from] ::redis::RedisError),

    /// A NATS connection error (`nats` feature).
    #[cfg(feature = "nats")]
    #[error("nats connection error: {0}")]
    Nats(#[from] async_nats::ConnectError),

    /// A RabbitMQ connection error (`rabbitmq` feature).
    #[cfg(feature = "rabbitmq")]
    #[error("rabbitmq connection error: {0}")]
    RabbitMq(#[from] lapin::Error),

    /// A Kafka connection error (`kafka` feature).
    #[cfg(feature = "kafka")]
    #[error("kafka connection error: {0}")]
    Kafka(#[from] rskafka::client::error::Error),

    /// An object-storage error (`storage` feature).
    #[cfg(feature = "storage")]
    #[error("storage error: {0}")]
    Storage(#[from] object_store::Error),

    /// A pagination request was malformed (e.g. an out-of-range page size).
    #[error("invalid pagination request: {0}")]
    InvalidPage(String),

    /// A pagination cursor was invalid or could not be decoded.
    #[error("invalid or corrupted cursor: {0}")]
    InvalidCursor(String),
}

impl DataError {
    /// Map a variant to its category and stable code. A wrapped [`ConfigError`]
    /// delegates to that error's own classification so codes stay accurate
    /// across crate boundaries.
    fn classify(&self) -> (ErrorCategory, ErrorCode) {
        use ErrorCategory::Internal;
        match self {
            DataError::Config(e) => (e.category(), e.code()),
            DataError::UnsupportedSystem(_) => {
                (Internal, ErrorCode::new("data.unsupported_system"))
            }
            DataError::UnsupportedCacheBackend(_) => {
                (Internal, ErrorCode::new("data.unsupported_cache_backend"))
            }
            DataError::UnsupportedMessagingBackend(_) => {
                (Internal, ErrorCode::new("data.unsupported_messaging_backend"))
            }
            DataError::MissingUrl(_) => (Internal, ErrorCode::new("data.missing_url")),
            DataError::Messaging(_) => (Internal, ErrorCode::new("data.messaging")),
            DataError::Outbox(_) => (Internal, ErrorCode::new("data.outbox")),
            DataError::Idempotency(_) => (Internal, ErrorCode::new("data.idempotency")),
            DataError::Migration(_) => (Internal, ErrorCode::new("data.migration")),
            // Another holder owns the lock — a conflict from the caller's view.
            DataError::LockHeld(_) => (ErrorCategory::Conflict, ErrorCode::new("data.lock_held")),
            DataError::FeatureDisabled(_) => (Internal, ErrorCode::new("data.feature_disabled")),
            DataError::Io(_) => (Internal, ErrorCode::new("data.io")),
            // Connection/transport failures are transient from the caller's view.
            #[cfg(feature = "sql")]
            DataError::Sqlx(_) => (ErrorCategory::Unavailable, ErrorCode::new("data.database")),
            #[cfg(feature = "redis")]
            DataError::Redis(_) => (ErrorCategory::Unavailable, ErrorCode::new("data.redis")),
            #[cfg(feature = "nats")]
            DataError::Nats(_) => (ErrorCategory::Unavailable, ErrorCode::new("data.nats")),
            #[cfg(feature = "rabbitmq")]
            DataError::RabbitMq(_) => (ErrorCategory::Unavailable, ErrorCode::new("data.rabbitmq")),
            #[cfg(feature = "kafka")]
            DataError::Kafka(_) => (ErrorCategory::Unavailable, ErrorCode::new("data.kafka")),
            #[cfg(feature = "storage")]
            DataError::Storage(_) => (Internal, ErrorCode::new("data.storage")),
            DataError::InvalidPage(_) => {
                (ErrorCategory::BadRequest, ErrorCode::new("data.invalid_page"))
            }
            DataError::InvalidCursor(_) => {
                (ErrorCategory::BadRequest, ErrorCode::new("data.invalid_cursor"))
            }
        }
    }
}

impl DomainError for DataError {
    fn category(&self) -> ErrorCategory {
        self.classify().0
    }

    fn code(&self) -> ErrorCode {
        self.classify().1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_system_is_internal() {
        let err = DataError::UnsupportedSystem(DbSystem::MongoDb);
        assert_eq!(err.category(), ErrorCategory::Internal);
        assert_eq!(err.code().as_str(), "data.unsupported_system");
    }

    #[test]
    fn wrapped_config_error_delegates_classification() {
        let err: DataError = ConfigError::MissingRequired("database".into()).into();
        // Category and code come from the inner ConfigError, not from data.
        assert_eq!(err.category(), ErrorCategory::Internal);
        assert_eq!(err.code().as_str(), "config.missing_required");
    }
}
