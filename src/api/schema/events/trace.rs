use async_graphql::Object;
use vector_lib::encode_logfmt;
use vector_lib::event;
use vector_lib::tap::topology::TapOutput;
use vrl::event_path;

use super::EventEncodingType;

#[derive(Debug, Clone)]
pub struct Trace {
    output: TapOutput,
    event: event::TraceEvent,
}

impl Trace {
    pub const fn new(output: TapOutput, event: event::TraceEvent) -> Self {
        Self { output, event }
    }
}

#[Object]
/// Trace event with fields for querying trace data
impl Trace {
    /// Id of the component associated with the trace event
    async fn component_id(&self) -> &str {
        self.output.output_id.component.id()
    }

    /// Type of component associated with the trace event
    async fn component_type(&self) -> &str {
        self.output.component_type.as_ref()
    }

    /// Kind of component associated with the trace event
    async fn component_kind(&self) -> &str {
        self.output.component_kind
    }

    /// Trace event as an encoded string format
    async fn string(&self, encoding: EventEncodingType) -> String {
        match encoding {
            EventEncodingType::Json => serde_json::to_string(&self.event)
                .expect("JSON serialization of log event failed. Please report."),
            EventEncodingType::Yaml => serde_yaml::to_string(&self.event)
                .expect("YAML serialization of log event failed. Please report."),
            EventEncodingType::Logfmt => encode_logfmt::encode_map(self.event.as_map())
                .expect("logfmt serialization of log event failed. Please report."),
        }
    }

    /// Get JSON field data on the trace event, by field name
    async fn json(&self, field: String) -> Option<String> {
        self.event.get(event_path!(field.as_str())).map(|field| {
            serde_json::to_string(field)
                .expect("JSON serialization of log event field failed. Please report.")
        })
    }
}
