#[cfg(feature = "sources-prometheus")]
use std::borrow::Cow;
use std::time::Instant;

use super::prelude::{error_stage, error_type, http_error_code};
use hyper::StatusCode;
use metrics::{counter, histogram};
#[cfg(feature = "sources-prometheus")]
use prometheus_parser::ParserError;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct PrometheusEventsReceived {
    pub byte_size: usize,
    pub count: usize,
    pub uri: http::Uri,
}

impl InternalEvent for PrometheusEventsReceived {
    fn emit(self) {
        trace!(
            message = "Events received.",
            count = %self.count,
            byte_size = %self.byte_size,
            uri = %self.uri,
        );
        counter!(
            "component_received_events_total", self.count as u64,
            "uri" => self.uri.to_string(),
        );
        counter!(
            "component_received_event_bytes_total", self.byte_size as u64,
            "uri" => self.uri.to_string(),
        );
        // deprecated
        counter!(
            "events_in_total", self.count as u64,
            "uri" => self.uri.to_string(),
        );
    }
}

#[derive(Debug)]
pub struct PrometheusRequestCompleted {
    pub start: Instant,
    pub(crate) end: Instant,
}

impl InternalEvent for PrometheusRequestCompleted {
    fn emit(self) {
        debug!(message = "Request completed.");
        counter!("requests_completed_total", 1);
        histogram!("request_duration_seconds", self.end - self.start);
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
    fn emit(self) {
        error!(
            message = "Parsing error.",
            url = %self.url,
            error = ?self.error,
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_secs = 10,
        );
        debug!(
            message = %format!("Failed to parse response:\n\n{}\n\n", self.body),
            url = %self.url,
            internal_log_rate_secs = 10
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
            "url" => self.url.to_string(),
        );
        // deprecated
        counter!("parse_errors_total", 1);
    }
}

#[derive(Debug)]
pub struct PrometheusHttpResponseError {
    pub code: hyper::StatusCode,
    pub url: http::Uri,
}

impl InternalEvent for PrometheusHttpResponseError {
    fn emit(self) {
        error!(
            message = "HTTP error response.",
            url = %self.url,
            stage = error_stage::RECEIVING,
            error_type = error_type::REQUEST_FAILED,
            error_code = %http_error_code(self.code.as_u16()),
            internal_log_rate_secs = 10,
        );
        counter!(
            "component_errors_total", 1,
            "url" => self.url.to_string(),
            "stage" => error_stage::RECEIVING,
            "error_type" => error_type::REQUEST_FAILED,
            "error_code" => http_error_code(self.code.as_u16()),
        );
        // deprecated
        counter!("http_error_response_total", 1);
    }
}

#[derive(Debug)]
pub struct PrometheusHttpError {
    pub error: crate::Error,
    pub url: http::Uri,
}

impl InternalEvent for PrometheusHttpError {
    fn emit(self) {
        error!(
            message = "HTTP request processing error.",
            url = %self.url,
            error = ?self.error,
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::RECEIVING,
            internal_log_rate_secs = 10,
        );
        counter!(
            "component_errors_total", 1,
            "url" => self.url.to_string(),
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::RECEIVING,
        );
        // deprecated
        counter!("http_request_errors_total", 1);
    }
}

#[derive(Debug)]
pub struct PrometheusRemoteWriteParseError {
    pub error: prost::DecodeError,
}

impl InternalEvent for PrometheusRemoteWriteParseError {
    fn emit(self) {
        error!(
            message = "Could not decode request body.",
            error = ?self.error,
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_secs = 10,
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!("parse_errors_total", 1);
    }
}

#[derive(Debug)]
pub struct PrometheusServerRequestComplete {
    pub status_code: StatusCode,
}

impl InternalEvent for PrometheusServerRequestComplete {
    fn emit(self) {
        let message = "Request to prometheus server complete.";
        if self.status_code.is_success() {
            debug!(message, status_code = %self.status_code);
        } else {
            warn!(message, status_code = %self.status_code);
        }
        counter!("requests_received_total", 1);
    }
}
