//! User notifications: the [`Notifier`] trait, a [`Notification`] / [`Channel`]
//! model, and a [`RecordingNotifier`] for tests and local development.
//!
//! Distinct from [`webhooks`](crate::webhooks) (which delivers system events to
//! endpoint URLs): notifications are user-facing messages (email / SMS / push) to
//! a recipient.

pub mod model;
pub mod notifier;

pub use model::{Channel, Notification};
pub use notifier::{Notifier, RecordingNotifier};
