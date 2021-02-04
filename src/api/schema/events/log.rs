use crate::event;
use async_graphql::Object;
use serde_json::json;

#[derive(Debug)]
/// A log event
pub struct LogEvent(event::LogEvent);

impl LogEvent {
    pub fn new(ev: event::LogEvent) -> Self {
        Self(ev)
    }
}

#[Object]
impl LogEvent {
    /// Log message
    async fn message(&self) -> Option<String> {
        serde_json::to_string(self.0.get("message")?).ok()
    }

    /// Get the log event as a JSON string
    async fn json(&self) -> String {
        json!(self.0).to_string()
    }

    /// Get the log event as a YAML string
    async fn yaml(&self) -> Option<String> {
        serde_yaml::to_string(&self.0).ok()
    }
}
