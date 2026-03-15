use vector_lib::{
    config::{DataType, SinkOutput},
    source_sender::SourceSender,
};
use vrl::path::ValuePath;
use vrl::{metadata_path, path};

use crate::{
    config::{ComponentKey, SinkContext, log_schema},
    event::{Event, LogEvent},
    schema::Definition,
};

/// Conventional sink output port name used for dead-letter routing.
pub const DLQ_OUTPUT: &str = "dlq";

#[derive(Clone)]
pub struct SinkDlq {
    component_id: ComponentKey,
    component_type: &'static str,
    output: SourceSender,
}

impl SinkDlq {
    pub fn from_context(cx: &SinkContext, component_type: &'static str) -> Option<Self> {
        let output = cx.outputs().cloned()?;
        let component_id = cx
            .key
            .clone()
            .unwrap_or_else(|| ComponentKey::from("unknown"));

        Some(Self {
            component_id,
            component_type,
            output,
        })
    }

    pub fn log_output() -> SinkOutput {
        SinkOutput::new_maybe_logs(DataType::Log, Definition::any()).with_port(DLQ_OUTPUT)
    }

    pub fn annotate_log(
        &self,
        log: &mut LogEvent,
        reason: &str,
        mut details: serde_json::Map<String, serde_json::Value>,
    ) {
        details.insert("reason".to_string(), reason.into());
        details.insert(
            "component_id".to_string(),
            self.component_id.to_string().into(),
        );
        details.insert(
            "component_type".to_string(),
            self.component_type.to_string().into(),
        );
        details.insert("component_kind".to_string(), "sink".into());

        let data = serde_json::Value::Object(details);

        match log.namespace() {
            vector_lib::config::LogNamespace::Legacy => {
                if let Some(metadata_key) = log_schema().metadata_key() {
                    use vrl::path::PathPrefix;
                    log.insert((PathPrefix::Event, metadata_key.concat(path!("dlq"))), data);
                }
            }
            vector_lib::config::LogNamespace::Vector => {
                log.insert(metadata_path!("vector", "dlq"), data);
            }
        }
    }

    pub async fn send_events(&mut self, events: Vec<Event>) -> Result<(), crate::Error> {
        if events.is_empty() {
            return Ok(());
        }

        // `send_batch_named` emits standard output metrics/events for the `dlq` port.
        self.output
            .send_batch_named(DLQ_OUTPUT, events)
            .await
            .map_err(Into::into)
    }
}
