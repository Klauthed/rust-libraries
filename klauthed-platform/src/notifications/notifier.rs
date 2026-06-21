//! The [`Notifier`] trait and a [`RecordingNotifier`].

use std::sync::Mutex;

use async_trait::async_trait;

use super::Notification;
use crate::error::PlatformError;

/// Delivers [`Notification`]s to users. Implement for an email/SMS/push provider,
/// mapping provider failures to [`PlatformError`]. Object-safe, so a notifier can
/// be shared as `Arc<dyn Notifier>`.
#[async_trait]
pub trait Notifier: Send + Sync {
    /// Deliver one notification.
    async fn send(&self, notification: &Notification) -> Result<(), PlatformError>;
}

/// A [`Notifier`] that records what would be sent instead of delivering it — for
/// tests and local development.
#[derive(Default)]
pub struct RecordingNotifier {
    sent: Mutex<Vec<Notification>>,
}

impl RecordingNotifier {
    /// A new, empty recording notifier.
    pub fn new() -> Self {
        Self::default()
    }

    /// A snapshot of all recorded notifications, in send order.
    pub fn sent(&self) -> Vec<Notification> {
        self.sent.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
    }

    /// The number of recorded notifications.
    pub fn len(&self) -> usize {
        self.sent.lock().unwrap_or_else(std::sync::PoisonError::into_inner).len()
    }

    /// Whether nothing has been recorded yet.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[async_trait]
impl Notifier for RecordingNotifier {
    async fn send(&self, notification: &Notification) -> Result<(), PlatformError> {
        self.sent
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(notification.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notifications::Channel;

    #[tokio::test]
    async fn records_sent_notifications_in_order() {
        let notifier = RecordingNotifier::new();
        assert!(notifier.is_empty());

        notifier.send(&Notification::email("a@b.com", "Welcome", "Hi there")).await.unwrap();
        notifier.send(&Notification::sms("+15550100", "Your code is 123")).await.unwrap();

        let sent = notifier.sent();
        assert_eq!(notifier.len(), 2);
        assert_eq!(sent[0].channel, Channel::Email);
        assert_eq!(sent[0].subject.as_deref(), Some("Welcome"));
        assert_eq!(sent[1].channel, Channel::Sms);
        assert_eq!(sent[1].subject, None);
        assert_eq!(sent[1].body, "Your code is 123");
    }
}
