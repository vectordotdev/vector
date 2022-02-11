use async_graphql::Object;
use vector_common::encode_logfmt;

use super::EventEncodingType;
use crate::{
    config::OutputId,
    event::{self, Value},
};

#[derive(Debug, Clone)]
pub struct Trace {
    output_id: OutputId,
    event: event::TraceEvent,
}

impl Trace {
    pub const fn new(output_id: OutputId, event: event::TraceEvent) -> Self {
        Self { output_id, event }
    }
}

#[Object]
/// Log event with fields for querying log data
impl Trace {
    /// Id of the component associated with the log event
    async fn component_id(&self) -> &str {
        self.output_id.component.id()
    }

    /// Trace event as an encoded string format
    async fn string(&self, encoding: EventEncodingType) -> String {
        match encoding {
            EventEncodingType::Json => serde_json::to_string(&self.event)
                .expect("JSON serialization of log event failed. Please report."),
            EventEncodingType::Yaml => serde_yaml::to_string(&self.event)
                .expect("YAML serialization of log event failed. Please report."),
            EventEncodingType::Logfmt => encode_logfmt::to_string(self.event.as_map())
                .expect("logfmt serialization of log event failed. Please report."),
        }
    }

    /// Get JSON field data on the trace event, by field name
    async fn json(&self, field: String) -> Option<&Value> {
        self.event.get(field)
    }
}
