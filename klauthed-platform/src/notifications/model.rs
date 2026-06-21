//! Notification model: a [`Notification`] to a recipient over a [`Channel`].

use serde::{Deserialize, Serialize};

/// The delivery channel for a [`Notification`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    /// Email.
    Email,
    /// SMS / text message.
    Sms,
    /// Mobile push notification.
    Push,
}

/// A user-facing message to deliver over a [`Channel`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notification {
    /// The delivery channel.
    pub channel: Channel,
    /// The recipient for the channel: an email address, phone number, or device token.
    pub recipient: String,
    /// The subject/title, where the channel has one (email subject, push title).
    pub subject: Option<String>,
    /// The message body.
    pub body: String,
}

impl Notification {
    /// An email notification with a subject.
    pub fn email(
        recipient: impl Into<String>,
        subject: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        Self {
            channel: Channel::Email,
            recipient: recipient.into(),
            subject: Some(subject.into()),
            body: body.into(),
        }
    }

    /// An SMS notification (no subject).
    pub fn sms(recipient: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            channel: Channel::Sms,
            recipient: recipient.into(),
            subject: None,
            body: body.into(),
        }
    }

    /// A push notification with a title.
    pub fn push(
        recipient: impl Into<String>,
        title: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        Self {
            channel: Channel::Push,
            recipient: recipient.into(),
            subject: Some(title.into()),
            body: body.into(),
        }
    }
}
