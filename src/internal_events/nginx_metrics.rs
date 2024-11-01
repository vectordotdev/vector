use metrics::counter;
use vector_lib::internal_event::InternalEvent;
use vector_lib::{
    internal_event::{error_stage, error_type},
    json_size::JsonSize,
};

use crate::sources::nginx_metrics::parser::ParseError;

#[derive(Debug)]
pub struct NginxMetricsEventsReceived<'a> {
    pub byte_size: JsonSize,
    pub count: usize,
    pub endpoint: &'a str,
}

impl<'a> InternalEvent for NginxMetricsEventsReceived<'a> {
    fn emit(self) {
        trace!(
            message = "Events received.",
            byte_size = %self.byte_size,
            count = %self.count,
            endpoint = self.endpoint,
        );
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

pub struct NginxMetricsRequestError<'a> {
    pub error: crate::Error,
    pub endpoint: &'a str,
}

impl<'a> InternalEvent for NginxMetricsRequestError<'a> {
    fn emit(self) {
        error!(
            message = "Nginx request error.",
            endpoint = %self.endpoint,
            error = %self.error,
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::RECEIVING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "endpoint" => self.endpoint.to_owned(),
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}

pub(crate) struct NginxMetricsStubStatusParseError<'a> {
    pub error: ParseError,
    pub endpoint: &'a str,
}

impl<'a> InternalEvent for NginxMetricsStubStatusParseError<'a> {
    fn emit(self) {
        error!(
            message = "NginxStubStatus parse error.",
            endpoint = %self.endpoint,
            error = %self.error,
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "endpoint" => self.endpoint.to_owned(),
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
    }
}
