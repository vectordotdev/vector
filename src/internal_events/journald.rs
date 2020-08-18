use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub(crate) struct JournaldEventReceived {
    pub byte_size: usize,
}

impl InternalEvent for JournaldEventReceived {
    fn emit_logs(&self) {
        trace!(message = "Received line.", byte_size = %self.byte_size);
    }

    fn emit_metrics(&self) {
        counter!("events_processed", 1,
                 "component_kind" => "source",
                 "component_name" => "journald",
        );
        counter!("bytes_processed", self.byte_size as u64,
                 "component_kind" => "source",
                 "component_name" => "journald",
        );
    }
}

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
