// ## skip check-events ##

use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub(crate) struct DatadogEventsProcessed {
    pub(crate) byte_size: usize,
}

impl InternalEvent for DatadogEventsProcessed {
    fn emit_metrics(&self) {
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

#[derive(Debug)]
pub(crate) struct DatadogEventsFieldInvalid {
    pub(crate) field: &'static str,
}

impl InternalEvent for DatadogEventsFieldInvalid {
    fn emit_logs(&self) {
        error!(
            message = "Required field is missing.",
            field = %self.field,
            internal_log_rate_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "processing_errors_total", 1,
            "error_type" => "field_missing",
            "field" => self.field);
    }
}
