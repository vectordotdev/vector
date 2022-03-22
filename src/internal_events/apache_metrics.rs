use std::time::Instant;

use super::prelude::{error_stage, error_type};
use metrics::{counter, histogram};
use vector_core::internal_event::InternalEvent;

use crate::sources::apache_metrics;

#[derive(Debug)]
pub struct ApacheMetricsEventsReceived<'a> {
    pub byte_size: usize,
    pub count: usize,
    pub endpoint: &'a str,
}

impl<'a> InternalEvent for ApacheMetricsEventsReceived<'a> {
    fn emit(self) {
        trace!(message = "Events received.", count = %self.count, byte_size = %self.byte_size, endpoint = %self.endpoint);
        counter!(
            "component_received_events_total", self.count as u64,
            "endpoint" => self.endpoint.to_owned(),
        );
        counter!(
            "component_received_event_bytes_total", self.byte_size as u64,
            "endpoint" => self.endpoint.to_owned(),
        );
        counter!(
            "events_in_total", self.count as u64,
            "uri" => self.endpoint.to_owned(),
        );
        counter!(
            "processed_bytes_total", self.byte_size as u64,
            "uri" => self.endpoint.to_owned(),
        );
    }
}

#[derive(Debug)]
pub struct ApacheMetricsRequestCompleted {
    pub start: Instant,
    pub end: Instant,
}

impl InternalEvent for ApacheMetricsRequestCompleted {
    fn emit(self) {
        debug!(message = "Request completed.");
        counter!("requests_completed_total", 1);
        histogram!("request_duration_seconds", self.end - self.start);
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
            endpoint = %self.endpoint,
            error = ?self.error,
            stage = error_stage::PROCESSING,
            error_type = error_type::PARSER_FAILED,
        );
        debug!(
            message = %format!("Parse error:\n\n{}\n\n", self.error),
            endpoint = %self.endpoint,
            internal_log_rate_secs = 10
        );
        counter!("parse_errors_total", 1);
        counter!(
            "component_errors_total", 1,
            "stage" => error_stage::PROCESSING,
            "error_type" => error_type::PARSER_FAILED,
            "endpoint" => self.endpoint.to_owned(),
        );
    }
}

#[derive(Debug)]
pub struct ApacheMetricsResponseError<'a> {
    pub code: hyper::StatusCode,
    pub endpoint: &'a str,
}

impl InternalEvent for ApacheMetricsResponseError<'_> {
    fn emit(self) {
        error!(
            message = "HTTP error response.",
            endpoint = %self.endpoint,
            code = %self.code,
            stage = error_stage::RECEIVING,
            error_type = error_type::REQUEST_FAILED,
            endpoint = %self.endpoint,
            error = %self.code,
        );
        counter!("http_error_response_total", 1);
        counter!(
            "component_errors_total", 1,
            "stage" => error_stage::RECEIVING,
            "error_type" => error_type::REQUEST_FAILED,
            "endpoint" => self.endpoint.to_owned(),
            "code" => self.code.to_string(),
        );
    }
}

#[derive(Debug)]
pub struct ApacheMetricsHttpError<'a> {
    pub error: crate::Error,
    pub endpoint: &'a str,
}

impl InternalEvent for ApacheMetricsHttpError<'_> {
    fn emit(self) {
        error!(
            message = "HTTP request processing error.",
            endpoint = %self.endpoint,
            error = ?self.error,
            stage = error_stage::RECEIVING,
            error_type = error_type::REQUEST_FAILED,
        );
        counter!("http_request_errors_total", 1);
        counter!(
            "component_errors_total", 1,
            "stage" => error_stage::RECEIVING,
            "error_type" => error_type::REQUEST_FAILED,
            "endpoint" => self.endpoint.to_owned(),
            "error" => self.error.to_string(),
        );
    }
}
