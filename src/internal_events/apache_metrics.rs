use super::InternalEvent;
use crate::sources::apache_metrics;
use http::Uri;
use metrics::{counter, histogram};
use std::time::Instant;

#[derive(Debug)]
pub struct ApacheMetricsEventReceived {
    pub byte_size: usize,
    pub count: usize,
}

impl InternalEvent for ApacheMetricsEventReceived {
    fn emit_logs(&self) {
        debug!(message = "Scraped events.", ?self.count);
    }

    fn emit_metrics(&self) {
        counter!("events_processed", self.count as u64);
        counter!("bytes_processed", self.byte_size as u64);
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
        counter!("requests_completed", 1);
        histogram!("request_duration_nanoseconds", self.end - self.start);
    }
}

#[derive(Debug)]
pub struct ApacheMetricsParseError {
    pub error: apache_metrics::ParseError,
    pub url: Uri,
}

impl InternalEvent for ApacheMetricsParseError {
    fn emit_logs(&self) {
        error!(message = "Parsing error.", url = %self.url, error = %self.error);
        debug!(
            message = %format!("Parse error:\n\n{}\n\n", self.error),
            url = %self.url,
            rate_limit_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!("parse_errors", 1);
    }
}

#[derive(Debug)]
pub struct ApacheMetricsErrorResponse {
    pub code: hyper::StatusCode,
    pub url: Uri,
}

impl InternalEvent for ApacheMetricsErrorResponse {
    fn emit_logs(&self) {
        error!(message = "HTTP error response.", url = %self.url, code = %self.code);
    }

    fn emit_metrics(&self) {
        counter!("http_error_response", 1);
    }
}

#[derive(Debug)]
pub struct ApacheMetricsHttpError {
    pub error: hyper::Error,
    pub url: Uri,
}

impl InternalEvent for ApacheMetricsHttpError {
    fn emit_logs(&self) {
        error!(message = "HTTP request processing error.", url = %self.url, error = %self.error);
    }

    fn emit_metrics(&self) {
        counter!("http_request_errors", 1);
    }
}
