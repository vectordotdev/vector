use async_graphql::Object;
use chrono::{DateTime, Utc};

use super::EventEncodingType;
use crate::{
    config::OutputId,
    event::{self, Value},
};

#[derive(Debug, Clone)]
pub struct Log {
    output_id: OutputId,
    event: event::LogEvent,
}

impl Log {
    pub const fn new(output_id: OutputId, event: event::LogEvent) -> Self {
        Self { output_id, event }
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
impl Log {
    /// Id of the component associated with the log event
    async fn component_id(&self) -> &str {
        self.output_id.component.id()
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
    async fn string(&self, encoding: EventEncodingType) -> String {
        match encoding {
            EventEncodingType::Json => serde_json::to_string(&self.event)
                .expect("JSON serialization of log event failed. Please report."),
            EventEncodingType::Yaml => serde_yaml::to_string(&self.event)
                .expect("YAML serialization of log event failed. Please report."),
        }
    }

    /// Get JSON field data on the log event, by field name
    async fn json(&self, field: String) -> Option<&Value> {
        self.event.get(field)
    }
}
