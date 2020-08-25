use super::InternalEvent;
use metrics::counter;

define_events_processed_bytes!(
    JournaldEventReceived,
    "source",
    "journald",
    "Received line."
);

#[derive(Debug)]
pub(crate) struct JournaldInvalidRecord {
    pub error: serde_json::Error,
    pub text: String,
}

impl InternalEvent for JournaldInvalidRecord {
    fn emit_logs(&self) {
        error!(message = "Invalid record from journald, discarding.", error = %self.error, text = %self.text);
    }

    fn emit_metrics(&self) {
        counter!("invalid_record", 1,
                 "component_kind" => "source",
                 "component_name" => "journald",
        );
        counter!("invalid_record_bytes", self.text.len() as u64,
                 "component_kind" => "source",
                 "component_name" => "journald",
        );
    }
}
