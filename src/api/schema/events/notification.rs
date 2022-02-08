use async_graphql::{Enum, SimpleObject};

#[derive(Enum, Debug, Copy, Clone, PartialEq, Eq)]
/// Event notification type
pub enum EventNotificationType {
    /// A component was found that matched the provided pattern
    Matched,
    /// There isn't currently a component that matches this pattern
    NotMatched,
}

#[derive(Debug, SimpleObject, Clone, PartialEq)]
/// A notification regarding events observation
pub struct EventNotification {
    /// Pattern that raised the event
    pattern: String,

    /// Event notification type
    notification: EventNotificationType,
}

impl EventNotification {
    pub const fn new(pattern: String, notification: EventNotificationType) -> Self {
        Self {
            pattern,
            notification,
        }
    }
}
