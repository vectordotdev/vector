use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct JournaldEventReceived {
    pub byte_size: usize,
}

impl InternalEvent for JournaldEventReceived {
    fn emit_logs(&self) {
        trace!(message = "Received line.", byte_size = %self.byte_size);
    }

    fn emit_metrics(&self) {
        counter!("received_events_total", 1);
        counter!("events_in_total", 1);
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct JournaldInvalidRecord {
    pub error: serde_json::Error,
    pub text: String,
}

impl InternalEvent for JournaldInvalidRecord {
    fn emit_logs(&self) {
        error!(message = "Invalid record from journald, discarding.", error = ?self.error, text = %self.text);
    }

    fn emit_metrics(&self) {
        counter!("invalid_record_total", 1);
        counter!("invalid_record_bytes_total", self.text.len() as u64);
    }
}
