use super::InternalEvent;
use metrics::{counter, histogram};
use std::borrow::Cow;
use std::time::Instant;

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
        counter!(
            "events_processed", self.count as u64,
            "component_kind" => "source",
            "component_type" => "aws_ecs_metrics",
        );
        counter!(
            "bytes_processed", self.byte_size as u64,
            "component_kind" => "source",
            "component_type" => "aws_ecs_metrics",
        );
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
        counter!("requests_completed", 1,
            "component_kind" => "source",
            "component_type" => "aws_ecs_metrics",
        );
        histogram!("request_duration_nanoseconds", self.end - self.start,
            "component_kind" => "source",
            "component_type" => "aws_ecs_metrics",
        );
    }
}

#[derive(Debug)]
pub struct AwsEcsMetricsParseError<'a> {
    pub error: serde_json::Error,
    pub url: String,
    pub body: Cow<'a, str>,
}

impl<'a> InternalEvent for AwsEcsMetricsParseError<'a> {
    fn emit_logs(&self) {
        error!(message = "Parsing error.", url = %self.url, error = %self.error);
        debug!(
            message = %format!("Failed to parse response:\n\n{}\n\n", self.body),
            url = %self.url,
            rate_limit_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!("parse_errors", 1,
            "component_kind" => "source",
            "component_type" => "aws_ecs_metrics",
        );
    }
}

#[derive(Debug)]
pub struct AwsEcsMetricsErrorResponse {
    pub code: hyper::StatusCode,
    pub url: String,
}

impl InternalEvent for AwsEcsMetricsErrorResponse {
    fn emit_logs(&self) {
        error!(message = "HTTP error response.", url = %self.url, code = %self.code);
    }

    fn emit_metrics(&self) {
        counter!("http_error_response", 1,
            "component_kind" => "source",
            "component_type" => "aws_ecs_metrics",
        );
    }
}

#[derive(Debug)]
pub struct AwsEcsMetricsHttpError {
    pub error: hyper::Error,
    pub url: String,
}

impl InternalEvent for AwsEcsMetricsHttpError {
    fn emit_logs(&self) {
        error!(message = "HTTP request processing error.", url = %self.url, error = %self.error);
    }

    fn emit_metrics(&self) {
        counter!("http_request_errors", 1,
            "component_kind" => "source",
            "component_type" => "aws_ecs_metrics",
        );
    }
}
