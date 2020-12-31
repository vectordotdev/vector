use super::InternalEvent;
use crate::sources::apache_metrics;
use metrics::{counter, histogram};
use std::time::Instant;

#[derive(Debug)]
pub struct ApacheMetricsEventReceived {
    pub byte_size: usize,
    pub count: usize,
}

impl InternalEvent for ApacheMetricsEventReceived {
    fn emit_logs(&self) {
        debug!(message = "Scraped events.", count = ?self.count);
    }

    fn emit_metrics(&self) {
        counter!("processed_events_total", self.count as u64);
        counter!("processed_bytes_total", self.byte_size as u64);
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
        histogram!("request_duration_nanoseconds", self.end - self.start);
    }
}

#[derive(Debug)]
pub struct ApacheMetricsParseError<'a> {
    pub error: apache_metrics::ParseError,
    pub url: &'a str,
}

impl InternalEvent for ApacheMetricsParseError<'_> {
    fn emit_logs(&self) {
        error!(message = "Parsing error.", url = %self.url, error = ?self.error);
        debug!(
            message = %format!("Parse error:\n\n{}\n\n", self.error),
            url = %self.url,
            internal_log_rate_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!("parse_errors_total", 1);
    }
}

#[derive(Debug)]
pub struct ApacheMetricsErrorResponse<'a> {
    pub code: hyper::StatusCode,
    pub url: &'a str,
}

impl InternalEvent for ApacheMetricsErrorResponse<'_> {
    fn emit_logs(&self) {
        error!(message = "HTTP error response.", url = %self.url, code = %self.code);
    }

    fn emit_metrics(&self) {
        counter!("http_error_response_total", 1);
    }
}

#[derive(Debug)]
pub struct ApacheMetricsHttpError<'a> {
    pub error: crate::Error,
    pub url: &'a str,
}

impl InternalEvent for ApacheMetricsHttpError<'_> {
    fn emit_logs(&self) {
        error!(message = "HTTP request processing error.", url = %self.url, error = ?self.error);
    }

    fn emit_metrics(&self) {
        counter!("http_request_errors_total", 1);
    }
}
