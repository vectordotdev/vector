use metrics::counter;
use vector_lib::{
    NamedInternalEvent,
    internal_event::{InternalEvent, error_stage, error_type},
    json_size::JsonSize,
};

use crate::sources::nginx_metrics::parser::ParseError;

#[derive(Debug, NamedInternalEvent)]
pub struct NginxMetricsEventsReceived<'a> {
    pub byte_size: JsonSize,
    pub count: usize,
    pub endpoint: &'a str,
}

impl InternalEvent for NginxMetricsEventsReceived<'_> {
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

#[derive(NamedInternalEvent)]
pub struct NginxMetricsRequestError<'a> {
    pub error: crate::Error,
    pub endpoint: &'a str,
}

impl InternalEvent for NginxMetricsRequestError<'_> {
    fn emit(self) {
        error!(
            message = "Nginx request error.",
            endpoint = %self.endpoint,
            error = %self.error,
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::RECEIVING,
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

#[derive(NamedInternalEvent)]
pub(crate) struct NginxMetricsStubStatusParseError<'a> {
    pub error: ParseError,
    pub endpoint: &'a str,
}

impl InternalEvent for NginxMetricsStubStatusParseError<'_> {
    fn emit(self) {
        error!(
            message = "NginxStubStatus parse error.",
            endpoint = %self.endpoint,
            error = %self.error,
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
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
