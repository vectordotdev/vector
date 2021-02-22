use crate::event;
use async_graphql::Object;
use chrono::{DateTime, Utc};
use serde_json::json;

#[derive(Debug)]
/// A log event
pub struct LogEvent(event::LogEvent);

impl LogEvent {
    pub fn new(event: event::LogEvent) -> Self {
        Self(event)
    }
}

#[Object]
impl LogEvent {
    /// Log message
    async fn message(&self) -> Option<String> {
        Some(self.0.get("message")?.to_string_lossy())
    }

    /// Log timestamp
    async fn timestamp(&self) -> Option<&DateTime<Utc>> {
        self.0.get("timestamp")?.as_timestamp()
    }

    /// Log event as a JSON string
    async fn json(&self) -> String {
        json!(self.0).to_string()
    }

    /// Log event as a YAML string
    async fn yaml(&self) -> Option<String> {
        serde_yaml::to_string(&self.0).ok()
    }
}
