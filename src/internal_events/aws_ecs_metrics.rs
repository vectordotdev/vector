use std::{borrow::Cow, time::Instant};

use metrics::{counter, histogram};
use vector_core::internal_event::InternalEvent;

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
    pub endpoint: &'a str,
    pub body: Cow<'a, str>,
}

impl<'a> InternalEvent for AwsEcsMetricsParseError<'_> {
    fn emit_logs(&self) {
        error!(
            message = "Parsing error.",
            endpoint = %self.endpoint,
            error = ?self.error,
            stage = "processing",
            error_type = "parse_failed",
        );
        debug!(
            message = %format!("Failed to parse response:\\n\\n{}\\n\\n", self.body.escape_debug()),
            endpoint = %self.endpoint,
            internal_log_rate_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!("parse_errors_total", 1);
        counter!(
            "component_errors_total", 1,
            "stage" => "processing",
            "error" => self.error.to_string(),
            "error_type" => "parse_failed",
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
    fn emit_logs(&self) {
        error!(
            message = "HTTP error response.",
            endpoint = %self.endpoint,
            stage = "receiving",
            error = %self.code,
            error_type = "http_error",
        );
    }

    fn emit_metrics(&self) {
        counter!("http_error_response_total", 1);
        counter!(
            "component_errors_total", 1,
            "stage" => "receiving",
            "error" => self.code.to_string(),
            "error_type" => "http_error",
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
            "error" => self.error.to_string(),
            "error_type" => "http_error",
            "endpoint" => self.endpoint.to_owned(),
        );
    }
}
