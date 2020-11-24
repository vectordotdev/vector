use super::InternalEvent;
use crate::sources::prometheus::parser::ParserError;
use metrics::{counter, histogram};
use std::borrow::Cow;
use std::time::Instant;

#[derive(Debug)]
pub struct PrometheusEventReceived {
    pub byte_size: usize,
    pub count: usize,
}

impl InternalEvent for PrometheusEventReceived {
    fn emit_logs(&self) {
        debug!(message = "Scraped events.", count = ?self.count);
    }

    fn emit_metrics(&self) {
        counter!("processed_events_total", self.count as u64);
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct PrometheusRequestCompleted {
    pub start: Instant,
    pub end: Instant,
}

impl InternalEvent for PrometheusRequestCompleted {
    fn emit_logs(&self) {
        debug!(message = "Request completed.");
    }

    fn emit_metrics(&self) {
        counter!("requests_completed_total", 1);
        histogram!("request_duration_nanoseconds", self.end - self.start);
    }
}

#[derive(Debug)]
pub struct PrometheusParseError<'a> {
    pub error: ParserError,
    pub url: http::Uri,
    pub body: Cow<'a, str>,
}

impl<'a> InternalEvent for PrometheusParseError<'a> {
    fn emit_logs(&self) {
        error!(message = "Parsing error.", url = %self.url, error = ?self.error);
        debug!(
            message = %format!("Failed to parse response:\n\n{}\n\n", self.body),
            url = %self.url,
            rate_limit_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!("parse_errors_total", 1);
    }
}

#[derive(Debug)]
pub struct PrometheusErrorResponse {
    pub code: hyper::StatusCode,
    pub url: http::Uri,
}

impl InternalEvent for PrometheusErrorResponse {
    fn emit_logs(&self) {
        error!(message = "HTTP error response.", url = %self.url, code = %self.code);
    }

    fn emit_metrics(&self) {
        counter!("http_error_response_total", 1);
    }
}

#[derive(Debug)]
pub struct PrometheusHttpError {
    pub error: crate::Error,
    pub url: http::Uri,
}

impl InternalEvent for PrometheusHttpError {
    fn emit_logs(&self) {
        error!(message = "HTTP request processing error.", url = %self.url, error = ?self.error);
    }

    fn emit_metrics(&self) {
        counter!("http_request_errors_total", 1);
    }
}
