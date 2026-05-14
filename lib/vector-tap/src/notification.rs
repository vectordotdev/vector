#[derive(Debug, Clone, PartialEq, Eq)]
/// A component was found that matched the provided pattern
pub struct Matched {
    message: String,
    /// Pattern that raised the notification
    pub pattern: String,
}

impl Matched {
    pub fn new(pattern: String) -> Self {
        Self {
            message: format!("[tap] Pattern '{pattern}' successfully matched."),
            pattern,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// There isn't currently a component that matches this pattern
pub struct NotMatched {
    message: String,
    /// Pattern that raised the notification
    pub pattern: String,
}

impl NotMatched {
    pub fn new(pattern: String) -> Self {
        Self {
            message: format!(
                "[tap] Pattern '{pattern}' failed to match: will retry on configuration reload."
            ),
            pattern,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// The pattern matched source(s) which cannot be tapped for inputs or sink(s)
/// which cannot be tapped for outputs
pub struct InvalidMatch {
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

#[derive(Debug, Clone, PartialEq, Eq)]
/// A specific kind of notification with additional details
pub enum Notification {
    Matched(Matched),
    NotMatched(NotMatched),
    InvalidMatch(InvalidMatch),
}

impl Notification {
    pub fn as_str(&self) -> &str {
        match self {
            Notification::Matched(n) => n.message.as_ref(),
            Notification::NotMatched(n) => n.message.as_ref(),
            Notification::InvalidMatch(n) => n.message.as_ref(),
        }
    }
}
