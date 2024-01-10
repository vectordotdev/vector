use async_graphql::{Object, SimpleObject, Union};

#[derive(Debug, Clone, PartialEq, Eq, SimpleObject)]
/// A component was found that matched the provided pattern
pub struct Matched {
    #[graphql(skip)]
    message: String,
    /// Pattern that raised the notification
    pub pattern: String,
}

impl Matched {
    pub fn new(pattern: String) -> Self {
        Self {
            message: format!("[tap] Pattern '{}' successfully matched.", pattern),
            pattern,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, SimpleObject)]
/// There isn't currently a component that matches this pattern
pub struct NotMatched {
    #[graphql(skip)]
    message: String,
    /// Pattern that raised the notification
    pub pattern: String,
}

impl NotMatched {
    pub fn new(pattern: String) -> Self {
        Self {
            message: format!(
                "[tap] Pattern '{}' failed to match: will retry on configuration reload.",
                pattern
            ),
            pattern,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, SimpleObject)]
/// The pattern matched source(s) which cannot be tapped for inputs or sink(s)
/// which cannot be tapped for outputs
pub struct InvalidMatch {
    #[graphql(skip)]
    message: String,
    /// Pattern that raised the notification
    pattern: String,
    /// Any invalid matches for the pattern
    invalid_matches: Vec<String>,
}

impl InvalidMatch {
    pub fn new(message: String, pattern: String, invalid_matches: Vec<String>) -> Self {
        Self {
            message,
            pattern,
            invalid_matches,
        }
    }
}

#[derive(Union, Debug, Clone, PartialEq, Eq)]
/// A specific kind of notification with additional details
pub enum Notification {
    Matched(Matched),
    NotMatched(NotMatched),
    InvalidMatch(InvalidMatch),
}

impl Notification {
    fn as_str(&self) -> &str {
        match self {
            Notification::Matched(n) => n.message.as_ref(),
            Notification::NotMatched(n) => n.message.as_ref(),
            Notification::InvalidMatch(n) => n.message.as_ref(),
        }
    }
}

/// This wrapper struct hoists `message` up from [`Notification`] for a more
/// natural querying experience. While ideally [`Notification`] would be a
/// GraphQL interface with a common `message` field, an interface cannot be
/// directly nested into the union of [`super::OutputEventsPayload`].
///
/// The GraphQL specification forbids such a nesting:
/// <http://spec.graphql.org/October2021/#sel-HAHdfFDABABkG3_I>
#[derive(Debug, Clone)]
pub struct EventNotification {
    pub notification: Notification,
}

#[Object]
/// A notification regarding events observation
impl EventNotification {
    /// Notification details
    async fn notification(&self) -> &Notification {
        &self.notification
    }

    /// The human-readable message associated with the notification
    async fn message(&self) -> &str {
        self.notification.as_str()
    }
}
