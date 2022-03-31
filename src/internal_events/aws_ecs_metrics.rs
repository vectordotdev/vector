use std::{borrow::Cow, time::Instant};

use super::prelude::{error_stage, error_type};
use metrics::{counter, histogram};
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct AwsEcsMetricsEventsReceived<'a> {
    pub byte_size: usize,
    pub count: usize,
    pub http_path: &'a str,
}

impl<'a> InternalEvent for AwsEcsMetricsEventsReceived<'a> {
    fn emit(self) {
        trace!(
            message = "Events received.",
            count = %self.count,
            byte_size = %self.byte_size,
            protocol = "http",
            http_path = %self.http_path,
        );
        counter!(
            "component_received_events_total", self.count as u64,
            "http_path" => self.http_path.to_string(),
        );
        counter!(
            "component_received_event_bytes_total", self.byte_size as u64,
            "http_path" => self.http_path.to_string(),
        );
        // deprecated
        counter!("events_in_total", self.count as u64);
    }
}

#[derive(Debug)]
pub struct AwsEcsMetricsRequestCompleted {
    pub start: Instant,
    pub end: Instant,
}

impl InternalEvent for AwsEcsMetricsRequestCompleted {
    fn emit(self) {
        debug!(message = "Request completed.");
        counter!("requests_completed_total", 1);
        histogram!("request_duration_seconds", self.end - self.start);
    }
}

#[derive(Debug)]
pub struct AwsEcsMetricsParseError<'a> {
    pub error: serde_json::Error,
    pub endpoint: &'a str,
    pub body: Cow<'a, str>,
}

impl<'a> InternalEvent for AwsEcsMetricsParseError<'_> {
    fn emit(self) {
        error!(
            message = "Parsing error.",
            endpoint = %self.endpoint,
            error = ?self.error,
            stage = error_stage::PROCESSING,
            error_type = error_type::PARSER_FAILED,
        );
        debug!(
            message = %format!("Failed to parse response:\\n\\n{}\\n\\n", self.body.escape_debug()),
            endpoint = %self.endpoint,
            internal_log_rate_secs = 10
        );
        counter!("parse_errors_total", 1);
        counter!(
            "component_errors_total", 1,
            "stage" => error_stage::PROCESSING,
            "error" => self.error.to_string(),
            "error_type" => error_type::PARSER_FAILED,
            "endpoint" => self.endpoint.to_owned(),
        );
    }
}

#[derive(Debug)]
pub struct AwsEcsMetricsResponseError<'a> {
    pub code: hyper::StatusCode,
    pub endpoint: &'a str,
}

impl InternalEvent for AwsEcsMetricsResponseError<'_> {
    fn emit(self) {
        error!(
            message = "HTTP error response.",
            endpoint = %self.endpoint,
            stage = error_stage::RECEIVING,
            error = %self.code,
            error_type = "http_error",
        );
        counter!("http_error_response_total", 1);
        counter!(
            "component_errors_total", 1,
            "stage" => error_stage::RECEIVING,
            "error" => self.code.to_string(),
            "error_type" => error_type::REQUEST_FAILED,
            "endpoint" => self.endpoint.to_owned(),
        );
    }
}

#[derive(Debug)]
pub struct AwsEcsMetricsHttpError<'a> {
    pub error: hyper::Error,
    pub endpoint: &'a str,
}

impl InternalEvent for AwsEcsMetricsHttpError<'_> {
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
            "error" => self.error.to_string(),
            "error_type" => error_type::REQUEST_FAILED,
            "endpoint" => self.endpoint.to_owned(),
        );
    }
}
