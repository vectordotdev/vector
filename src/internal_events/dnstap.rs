use super::prelude::{error_stage, error_type};
use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct DnstapEventsReceived {
    pub byte_size: usize,
}

impl InternalEvent for DnstapEventsReceived {
    fn emit(self) {
        trace!(message = "Events received.", count = 1, byte_size = %self.byte_size);
        counter!("component_received_events_total", 1);
        counter!(
            "component_received_event_bytes_total",
            self.byte_size as u64
        );
        // deprecated
        counter!("processed_events_total", 1);
        counter!("events_in_total", 1);
    }
}

#[derive(Debug)]
pub(crate) struct DnstapParseError<'a> {
    pub error: &'a str,
}

impl<'a> InternalEvent for DnstapParseError<'a> {
    fn emit(self) {
        error!(
            message = "Error occurred while parsing dnstap data.",
            error = ?self.error,
            stage = error_stage::PROCESSING,
            error_type = error_type::PARSER_FAILED,
            internal_log_rate_secs = 10,
        );
        counter!(
            "component_errors_total", 1,
            "stage" => error_stage::PROCESSING,
            "error_type" => error_type::PARSER_FAILED,
        );
        // deprecated
        counter!("parse_errors_total", 1);
    }
}
