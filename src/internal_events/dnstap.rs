use super::prelude::{error_stage, error_type};
use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct DnstapBytesReceived<'a> {
    pub byte_size: usize,
    pub endpoint: &'a str,
}

impl InternalEvent for DnstapBytesReceived<'_> {
    fn emit(self) {
        trace!(
            message = "Bytes received.",
            byte_size = %self.byte_size,
            protocol = "protobuf",
            endpoint = %self.endpoint,
        );
        counter!(
            "component_received_bytes_total", self.byte_size as u64,
            "protocol" => "protobuf",
            "endpoint" => self.endpoint.to_owned(),
        );
    }
}

#[derive(Debug)]
pub struct DnstapEventsReceived<'a> {
    pub byte_size: usize,
    pub endpoint: &'a str,
}

impl<'a> InternalEvent for DnstapEventsReceived<'a> {
    fn emit(self) {
        trace!(message = "Events received.", count = 1, byte_size = %self.byte_size, endpoint = self.endpoint);
        counter!("component_received_events_total", 1, "endpoint" => self.endpoint.to_owned());
        counter!(
            "component_received_event_bytes_total",
            self.byte_size as u64,
            "endpoint" => self.endpoint.to_owned(),
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
