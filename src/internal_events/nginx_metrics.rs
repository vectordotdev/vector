// ## skip check-events ##

use std::time::Instant;

use metrics::{counter, histogram};
use vector_core::internal_event::InternalEvent;

use crate::sources::nginx_metrics::parser::ParseError;

#[derive(Debug)]
pub struct NginxMetricsEventsReceived<'a> {
    pub byte_size: usize,
    pub count: usize,
    pub uri: &'a str,
}

impl<'a> InternalEvent for NginxMetricsEventsReceived<'a> {
    fn emit_logs(&self) {
        trace!(
            message = "Events received.",
            byte_size = %self.byte_size,
            count = %self.count,
            uri = self.uri,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_received_events_total", self.count as u64,
            "uri" => self.uri.to_owned(),
        );
        counter!(
            "component_received_event_bytes_total", self.byte_size as u64,
            "uri" => self.uri.to_owned(),
        );
        // deprecated
        counter!(
            "events_in_total", self.count as u64,
            "uri" => self.uri.to_owned(),
        );
    }
}

#[derive(Debug)]
pub struct NginxMetricsCollectCompleted {
    pub start: Instant,
    pub end: Instant,
}

impl InternalEvent for NginxMetricsCollectCompleted {
    fn emit_logs(&self) {
        debug!(message = "Collection completed.");
    }

    fn emit_metrics(&self) {
        counter!("collect_completed_total", 1);
        histogram!("collect_duration_seconds", self.end - self.start);
    }
}

pub struct NginxMetricsRequestError<'a> {
    pub error: crate::Error,
    pub endpoint: &'a str,
}

impl<'a> InternalEvent for NginxMetricsRequestError<'a> {
    fn emit_logs(&self) {
        error!(
            message = "Nginx request error.",
            endpoint = %self.endpoint,
            error = %self.error,
            error_type = "request_failed",
            stage = "receiving",
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "endpoint" => self.endpoint.to_owned(),
            "error" => self.error.to_string(),
            "error_type" => "request_failed",
            "stage" => "receiving",
        );
        // deprecated
        counter!("http_request_errors_total", 1);
    }
}

pub struct NginxMetricsStubStatusParseError<'a> {
    pub error: ParseError,
    pub endpoint: &'a str,
}

impl<'a> InternalEvent for NginxMetricsStubStatusParseError<'a> {
    fn emit_logs(&self) {
        error!(
            message = "NginxStubStatus parse error.",
            endpoint = %self.endpoint,
            error = %self.error,
            error_type = "parse_failed",
            stage = "processing",
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "endpoint" => self.endpoint.to_owned(),
            "error" => self.error.to_string(),
            "error_type" => "parse_failed",
            "stage" => "processing",
        );
        // deprecated
        counter!("parse_errors_total", 1);
    }
}
