// ## skip check-events ##

use std::{borrow::Cow, time::Instant};

use metrics::{counter, histogram};
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct AwsEcsMetricsReceived {
    pub byte_size: usize,
    pub count: usize,
}

impl InternalEvent for AwsEcsMetricsReceived {
    fn emit_logs(&self) {
        debug!(message = "Scraped events.", ?self.count);
    }

    fn emit_metrics(&self) {
        counter!("component_received_events_total", self.count as u64);
        counter!("events_in_total", self.count as u64);
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct AwsEcsMetricsRequestCompleted {
    pub start: Instant,
    pub end: Instant,
}

impl InternalEvent for AwsEcsMetricsRequestCompleted {
    fn emit_logs(&self) {
        debug!(message = "Request completed.");
    }

    fn emit_metrics(&self) {
        counter!("requests_completed_total", 1);
        histogram!("request_duration_seconds", self.end - self.start);
    }
}

#[derive(Debug)]
pub struct AwsEcsMetricsParseError<'a> {
    pub error: serde_json::Error,
    pub url: &'a str,
    pub body: Cow<'a, str>,
}

impl<'a> InternalEvent for AwsEcsMetricsParseError<'_> {
    fn emit_logs(&self) {
        error!(message = "Parsing error.", url = %self.url, error = %self.error);
        debug!(
            message = %format!("Failed to parse response:\\n\\n{}\\n\\n", self.body.escape_debug()),
            url = %self.url,
            internal_log_rate_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!("parse_errors_total", 1);
    }
}

#[derive(Debug)]
pub struct AwsEcsMetricsErrorResponse<'a> {
    pub code: hyper::StatusCode,
    pub url: &'a str,
}

impl InternalEvent for AwsEcsMetricsErrorResponse<'_> {
    fn emit_logs(&self) {
        error!(message = "HTTP error response.", url = %self.url, code = %self.code);
    }

    fn emit_metrics(&self) {
        counter!("http_error_response_total", 1);
    }
}

#[derive(Debug)]
pub struct AwsEcsMetricsHttpError<'a> {
    pub error: hyper::Error,
    pub url: &'a str,
}

impl InternalEvent for AwsEcsMetricsHttpError<'_> {
    fn emit_logs(&self) {
        error!(message = "HTTP request processing error.", url = %self.url, error = %self.error);
    }

    fn emit_metrics(&self) {
        counter!("http_request_errors_total", 1);
    }
}
