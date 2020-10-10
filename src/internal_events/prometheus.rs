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
        debug!(message = "Scraped events.", ?self.count);
    }

    fn emit_metrics(&self) {
        counter!("events_processed", self.count as u64);
        counter!("bytes_processed", self.byte_size as u64);
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
        counter!("requests_completed", 1);
        histogram!("request_duration_nanoseconds", self.end - self.start);
    }
}

#[derive(Debug)]
pub struct PrometheusParseError<'a> {
    pub error: ParserError,
    pub url: String,
    pub body: Cow<'a, str>,
}

impl<'a> InternalEvent for PrometheusParseError<'a> {
    fn emit_logs(&self) {
        error!(message = "Parsing error.", url = %self.url, error = %self.error);
        debug!(
            message = %format!("Failed to parse response:\n\n{}\n\n", self.body),
            url = %self.url,
            rate_limit_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!("parse_errors", 1);
    }
}

#[derive(Debug)]
pub struct PrometheusErrorResponse {
    pub code: hyper::StatusCode,
    pub url: String,
}

impl InternalEvent for PrometheusErrorResponse {
    fn emit_logs(&self) {
        error!(message = "HTTP error response.", url = %self.url, code = %self.code);
    }

    fn emit_metrics(&self) {
        counter!("http_error_response", 1);
    }
}

#[derive(Debug)]
pub struct PrometheusHttpError {
    pub error: hyper::Error,
    pub url: String,
}

impl InternalEvent for PrometheusHttpError {
    fn emit_logs(&self) {
        error!(message = "HTTP request processing error.", url = %self.url, error = %self.error);
    }

    fn emit_metrics(&self) {
        counter!("http_request_errors", 1);
    }
}
