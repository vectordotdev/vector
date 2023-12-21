use std::borrow::Cow;

use metrics::counter;
use vector_lib::{
    internal_event::{error_stage, error_type, InternalEvent},
    json_size::JsonSize,
};

#[derive(Debug)]
pub struct AwsEcsMetricsEventsReceived<'a> {
    pub byte_size: JsonSize,
    pub count: usize,
    pub endpoint: &'a str,
}

impl<'a> InternalEvent for AwsEcsMetricsEventsReceived<'a> {
    fn emit(self) {
        trace!(
            message = "Events received.",
            count = %self.count,
            byte_size = %self.byte_size,
            protocol = "http",
            endpoint = %self.endpoint,
        );
        counter!(
            "component_received_events_total", self.count as u64,
            "endpoint" => self.endpoint.to_string(),
        );
        counter!(
            "component_received_event_bytes_total", self.byte_size.get() as u64,
            "endpoint" => self.endpoint.to_string(),
        );
    }
}

#[derive(Debug)]
pub struct AwsEcsMetricsParseError<'a> {
    pub error: serde_json::Error,
    pub endpoint: &'a str,
    pub body: Cow<'a, str>,
}

impl<'a> InternalEvent for AwsEcsMetricsParseError<'a> {
    fn emit(self) {
        error!(
            message = "Parsing error.",
            endpoint = %self.endpoint,
            error = ?self.error,
            stage = error_stage::PROCESSING,
            error_type = error_type::PARSER_FAILED,
            internal_log_rate_limit = true,
        );
        debug!(
            message = %format!("Failed to parse response:\\n\\n{}\\n\\n", self.body.escape_debug()),
            endpoint = %self.endpoint,
            internal_log_rate_limit = true,
        );
        counter!("parse_errors_total", 1);
        counter!(
            "component_errors_total", 1,
            "stage" => error_stage::PROCESSING,
            "error_type" => error_type::PARSER_FAILED,
            "endpoint" => self.endpoint.to_string(),
        );
    }
}
