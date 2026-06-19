//! Messaging / event-bus connections from a
//! [`MessagingConfig`](klauthed_core::config::MessagingConfig).
//!
//! Each broker lives behind its own feature and contributes one connect
//! function. Because `MessagingConfig` is broker-tagged, every connector first
//! checks that the config selects its backend and otherwise returns
//! [`DataError::UnsupportedMessagingBackend`](crate::DataError) — so switching
//! brokers is a config + feature change, not a code rewrite.

#[cfg(feature = "nats")]
pub mod nats;

#[cfg(feature = "rabbitmq")]
pub mod rabbitmq;

#[cfg(feature = "kafka")]
pub mod kafka;

// Each backend's connector is `<backend>::connect`; re-exported here with a
// backend-qualified name so callers write `messaging::connect_nats(&config)`.
#[cfg(feature = "nats")]
pub use nats::connect as connect_nats;

#[cfg(feature = "rabbitmq")]
pub use rabbitmq::connect as connect_rabbitmq;

#[cfg(feature = "kafka")]
pub use kafka::connect as connect_kafka;
