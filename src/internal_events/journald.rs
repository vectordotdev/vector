use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct JournaldEventsReceived {
    pub count: usize,
    pub byte_size: usize,
}

impl InternalEvent for JournaldEventsReceived {
    fn emit_logs(&self) {
        trace!(message = "Received events.", count = %self.count, byte_size = %self.byte_size);
    }

    fn emit_metrics(&self) {
        counter!("component_received_events_total", self.count as u64);
        counter!(
            "component_received_event_bytes_total",
            self.byte_size as u64
        );
        counter!("events_in_total", self.count as u64); // deprecated
        counter!("processed_bytes_total", self.byte_size as u64); // deprecated
    }
}

#[derive(Debug)]
pub struct JournaldInvalidRecordError {
    pub error: serde_json::Error,
    pub text: String,
}

impl InternalEvent for JournaldInvalidRecordError {
    fn emit_logs(&self) {
        error!(
            message = "Invalid record from journald, discarding.",
            error = ?self.error,
            text = %self.text,
            stage = "processing",
            error_type = "parse_failed",
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "stage" => "processing",
            "error_type" => "parse_failed",
        );
        counter!("invalid_record_total", 1); // deprecated
        counter!("invalid_record_bytes_total", self.text.len() as u64); // deprecated
    }
}
