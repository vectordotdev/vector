use super::InternalEvent;
use hyper::StatusCode;
use metrics::{counter, histogram};
#[cfg(feature = "sources-prometheus")]
use prometheus_parser::ParserError;
#[cfg(feature = "sources-prometheus")]
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

#[cfg(feature = "sources-prometheus")]
#[derive(Debug)]
pub struct PrometheusParseError<'a> {
    pub error: ParserError,
    pub url: http::Uri,
    pub body: Cow<'a, str>,
}

#[cfg(feature = "sources-prometheus")]
impl<'a> InternalEvent for PrometheusParseError<'a> {
    fn emit_logs(&self) {
        error!(message = "Parsing error.", url = %self.url, error = ?self.error);
        debug!(
            message = %format!("Failed to parse response:\n\n{}\n\n", self.body),
            url = %self.url,
            internal_log_rate_secs = 10
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

#[derive(Debug)]
pub struct PrometheusRemoteWriteParseError {
    pub error: prost::DecodeError,
}

impl InternalEvent for PrometheusRemoteWriteParseError {
    fn emit_logs(&self) {
        error!(message = "Could not decode request body.", error = ?self.error);
    }

    fn emit_metrics(&self) {
        counter!("parse_errors_total", 1);
    }
}

#[derive(Debug)]
pub struct PrometheusRemoteWriteReceived {
    pub count: usize,
}

impl InternalEvent for PrometheusRemoteWriteReceived {
    fn emit_logs(&self) {
        debug!(message = "Received remote_write events.", count = ?self.count);
    }
}

#[derive(Debug)]
pub struct PrometheusNoNameError;

impl InternalEvent for PrometheusNoNameError {
    fn emit_logs(&self) {
        error!(
            message = "Decoded timeseries is missing the __name__ field.",
            internal_log_rate_secs = 5
        );
    }

    fn emit_metrics(&self) {
        counter!("parse_errors_total", 1);
    }
}

#[derive(Debug)]
pub struct PrometheusServerRequestComplete {
    pub status_code: StatusCode,
}

impl InternalEvent for PrometheusServerRequestComplete {
    fn emit_logs(&self) {
        let message = "Request to prometheus server complete.";
        if self.status_code.is_success() {
            debug!(message, status_code = %self.status_code);
        } else {
            error!(message, status_code = %self.status_code);
        }
    }

    fn emit_metrics(&self) {
        counter!("requests_received_total", 1);
    }
}

#[derive(Debug)]
pub struct PrometheusTemplateRenderingError {
    pub fields: Vec<String>,
}

impl InternalEvent for PrometheusTemplateRenderingError {
    fn emit_logs(&self) {
        error!(
            message = "Failed to render templated value; discarding value.",
            fields = ?self.fields,
            internal_log_rate_secs = 30,
        );
    }

    fn emit_metrics(&self) {
        counter!("processing_errors_total", 1);
    }
}
