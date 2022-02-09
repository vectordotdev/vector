use std::time::Instant;

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
    fn emit_logs(&self) {
        trace!(message = "Events received.", count = %self.count, byte_size = %self.byte_size, endpoint = %self.endpoint);
    }

    fn emit_metrics(&self) {
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
    fn emit_logs(&self) {
        debug!(message = "Request completed.");
    }

    fn emit_metrics(&self) {
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
    fn emit_logs(&self) {
        error!(
            message = "Parsing error.",
            endpoint = %self.endpoint,
            error = ?self.error,
            stage = "processing",
            error_type = "parse_failed",
        );
        debug!(
            message = %format!("Parse error:\n\n{}\n\n", self.error),
            endpoint = %self.endpoint,
            internal_log_rate_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!("parse_errors_total", 1);
        counter!(
            "component_errors_total", 1,
            "stage" => "processing",
            "error_type" => "parse_failed",
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
    fn emit_logs(&self) {
        error!(
            message = "HTTP error response.",
            endpoint = %self.endpoint,
            code = %self.code,
            stage = "receiving",
            error_type = "http_error",
            endpoint = %self.endpoint,
            error = %self.code,
        );
    }

    fn emit_metrics(&self) {
        counter!("http_error_response_total", 1);
        counter!(
            "component_errors_total", 1,
            "stage" => "receiving",
            "error_type" => "http_error",
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
    fn emit_logs(&self) {
        error!(
            message = "HTTP request processing error.",
            endpoint = %self.endpoint,
            error = ?self.error,
            stage = "receiving",
            error_type = "http_error",
        );
    }

    fn emit_metrics(&self) {
        counter!("http_request_errors_total", 1);
        counter!(
            "component_errors_total", 1,
            "stage" => "receiving",
            "error_type" => "http_error",
            "endpoint" => self.endpoint.to_owned(),
            "error" => self.error.to_string(),
        );
    }
}
