use metrics::counter;

use vector_lib::internal_event::InternalEvent;
use vector_lib::{
    internal_event::{error_stage, error_type},
    json_size::JsonSize,
};

use crate::sources::apache_metrics;

#[derive(Debug)]
pub struct ApacheMetricsEventsReceived<'a> {
    pub byte_size: JsonSize,
    pub count: usize,
    pub endpoint: &'a str,
}

impl<'a> InternalEvent for ApacheMetricsEventsReceived<'a> {
    // ## skip check-duplicate-events ##
    fn emit(self) {
        trace!(message = "Events received.", count = %self.count, byte_size = %self.byte_size, endpoint = %self.endpoint);
        counter!(
            "component_received_events_total",
            "endpoint" => self.endpoint.to_owned(),
        )
        .increment(self.count as u64);
        counter!(
            "component_received_event_bytes_total",
            "endpoint" => self.endpoint.to_owned(),
        )
        .increment(self.byte_size.get() as u64);
    }
}

#[derive(Debug)]
pub struct ApacheMetricsParseError<'a> {
    pub error: apache_metrics::ParseError,
    pub endpoint: &'a str,
}

impl InternalEvent for ApacheMetricsParseError<'_> {
    fn emit(self) {
        error!(
            message = "Parsing error.",
            error = ?self.error,
            stage = error_stage::PROCESSING,
            error_type = error_type::PARSER_FAILED,
            endpoint = %self.endpoint,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "stage" => error_stage::PROCESSING,
            "error_type" => error_type::PARSER_FAILED,
            "endpoint" => self.endpoint.to_owned(),
        )
        .increment(1);
    }
}
