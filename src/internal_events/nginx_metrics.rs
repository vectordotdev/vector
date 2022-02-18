use std::time::Instant;

use super::prelude::{error_stage, error_type};
use metrics::{counter, histogram};
use vector_core::internal_event::InternalEvent;

use crate::sources::nginx_metrics::parser::ParseError;

#[derive(Debug)]
pub(crate) struct NginxMetricsEventsReceived<'a> {
    pub(crate) byte_size: usize,
    pub(crate) count: usize,
    pub(crate) uri: &'a str,
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
pub(crate) struct NginxMetricsCollectCompleted {
    pub(crate) start: Instant,
    pub(crate) end: Instant,
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

pub(crate) struct NginxMetricsRequestError<'a> {
    pub(crate) error: crate::Error,
    pub(crate) endpoint: &'a str,
}

impl<'a> InternalEvent for NginxMetricsRequestError<'a> {
    fn emit_logs(&self) {
        error!(
            message = "Nginx request error.",
            endpoint = %self.endpoint,
            error = %self.error,
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::RECEIVING,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "endpoint" => self.endpoint.to_owned(),
            "error" => self.error.to_string(),
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::RECEIVING,
        );
        // deprecated
        counter!("http_request_errors_total", 1);
    }
}

pub(crate) struct NginxMetricsStubStatusParseError<'a> {
    pub(crate) error: ParseError,
    pub(crate) endpoint: &'a str,
}

impl<'a> InternalEvent for NginxMetricsStubStatusParseError<'a> {
    fn emit_logs(&self) {
        error!(
            message = "NginxStubStatus parse error.",
            endpoint = %self.endpoint,
            error = %self.error,
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "endpoint" => self.endpoint.to_owned(),
            "error" => self.error.to_string(),
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!("parse_errors_total", 1);
    }
}
