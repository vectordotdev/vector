use async_graphql::{Enum, SimpleObject};

#[derive(Enum, Debug, Copy, Clone, PartialEq, Eq)]
/// Event notification type
pub enum EventNotificationType {
    /// A component was found that matched the provided pattern
    Matched,
    /// There isn't currently a component that matches this pattern
    NotMatched,
    /// The input pattern matched source(s) which cannot be tapped for inputs
    InvalidInputPatternMatch,
    /// The output pattern matched sink(s) which cannot be tapped for outputs
    InvalidOutputPatternMatch,
}

#[derive(Debug, SimpleObject, Clone, PartialEq)]
/// A notification regarding events observation
pub struct EventNotification {
    /// Pattern that raised the event
    pattern: String,

    /// Event notification type
    notification: EventNotificationType,

    /// Any invalid matches for the pattern
    invalid_matches: Option<Vec<String>>,
}

impl EventNotification {
    pub const fn new(pattern: String, notification: EventNotificationType) -> Self {
        Self {
            pattern,
            notification,
            invalid_matches: None,
        }
    }

    pub const fn new_with_invalid_matches(
        pattern: String,
        notification: EventNotificationType,
        invalid_matches: Vec<String>,
    ) -> Self {
        Self {
            pattern,
            notification,
            invalid_matches: Some(invalid_matches),
        }
    }
}
