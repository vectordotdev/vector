use crate::event;
use async_graphql::Object;
use chrono::{DateTime, Utc};
use serde_json::json;

#[derive(Debug)]
pub struct LogEvent {
    component_name: String,
    event: event::LogEvent,
}

impl LogEvent {
    pub fn new(component_name: &str, event: event::LogEvent) -> Self {
        Self {
            component_name: component_name.to_string(),
            event,
        }
    }
}

#[Object]
/// Log event contains
impl LogEvent {
    /// Name of the component associated with the log event
    async fn component_name(&self) -> &str {
        &self.component_name
    }

    /// Log message
    async fn message(&self) -> Option<String> {
        Some(self.event.get("message")?.to_string_lossy())
    }

    /// Log timestamp
    async fn timestamp(&self) -> Option<&DateTime<Utc>> {
        self.event.get("timestamp")?.as_timestamp()
    }

    /// Log event as a JSON string
    async fn json(&self) -> String {
        json!(self.event).to_string()
    }

    /// Log event as a YAML string
    async fn yaml(&self) -> Option<String> {
        serde_yaml::to_string(&self.event).ok()
    }
}
