use crate::event;
use async_graphql::{Enum, Object};
use chrono::{DateTime, Utc};

#[derive(Enum, Copy, Clone, PartialEq, Eq)]
pub enum LogEventEncodingType {
    Json,
    Yaml,
}

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

    pub fn get_message(&self) -> Option<String> {
        Some(self.event.get("message")?.to_string_lossy())
    }

    pub fn get_timestamp(&self) -> Option<&DateTime<Utc>> {
        self.event.get("timestamp")?.as_timestamp()
    }
}

#[Object]
/// Log event with fields for querying log data
impl LogEvent {
    /// Name of the component associated with the log event
    async fn component_name(&self) -> &str {
        &self.component_name
    }

    /// Log message
    async fn message(&self) -> Option<String> {
        self.get_message()
    }

    /// Log timestamp
    async fn timestamp(&self) -> Option<&DateTime<Utc>> {
        self.get_timestamp()
    }

    /// Log event as an encoded string format
    async fn string(&self, encoding: LogEventEncodingType) -> Option<String> {
        match encoding {
            LogEventEncodingType::Json => serde_json::to_string(&self.event).ok(),
            LogEventEncodingType::Yaml => serde_yaml::to_string(&self.event).ok(),
        }
    }
}
