use super::prelude::{error_stage, error_type};
use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct DnstapEventsReceived {
    pub byte_size: usize,
}

impl InternalEvent for DnstapEventsReceived {
    fn emit_logs(&self) {
        trace!(message = "Events received.", count = 1, byte_size = %self.byte_size);
    }

    fn emit_metrics(&self) {
        counter!("component_received_events_total", 1);
        counter!(
            "component_received_event_bytes_total",
            self.byte_size as u64
        );
        // deprecated
        counter!("processed_events_total", 1);
        counter!("events_in_total", 1);
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

#[derive(Debug)]
pub(crate) struct DnstapParseError<'a> {
    pub error: &'a str,
}

impl<'a> InternalEvent for DnstapParseError<'a> {
    fn emit_logs(&self) {
        error!(
            message = "Error occurred while parsing dnstap data.",
            error = ?self.error,
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_secs = 10,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "stage" => error_stage::PROCESSING,
            "error" => self.error.to_string(),
            "error_type" => error_type::PARSER_FAILED,
        );
        // deprecated
        counter!("parse_errors_total", 1);
    }
}
