use std::time::Instant;

use super::prelude::{error_stage, error_type};
use metrics::{counter, histogram};
use vector_core::internal_event::InternalEvent;

use crate::sources::nginx_metrics::parser::ParseError;

#[derive(Debug)]
pub struct NginxMetricsEventsReceived<'a> {
    pub byte_size: usize,
    pub count: usize,
    pub endpoint: &'a str,
}

impl<'a> InternalEvent for NginxMetricsEventsReceived<'a> {
    fn emit(self) {
        trace!(
            message = "Events received.",
            byte_size = %self.byte_size,
            count = %self.count,
            endpoint = self.endpoint,
        );
        counter!(
            "component_received_events_total", self.count as u64,
            "endpoint" => self.endpoint.to_owned(),
        );
        counter!(
            "component_received_event_bytes_total", self.byte_size as u64,
            "endpoint" => self.endpoint.to_owned(),
        );
        // deprecated
        counter!(
            "events_in_total", self.count as u64,
            "endpoint" => self.endpoint.to_owned(),
        );
    }
}

#[derive(Debug)]
pub struct NginxMetricsCollectCompleted {
    pub start: Instant,
    pub end: Instant,
}

impl InternalEvent for NginxMetricsCollectCompleted {
    fn emit(self) {
        debug!(message = "Collection completed.");
        counter!("collect_completed_total", 1);
        histogram!("collect_duration_seconds", self.end - self.start);
    }
}

pub struct NginxMetricsRequestError<'a> {
    pub error: crate::Error,
    pub endpoint: &'a str,
}

impl<'a> InternalEvent for NginxMetricsRequestError<'a> {
    fn emit(self) {
        error!(
            message = "Nginx request error.",
            endpoint = %self.endpoint,
            error = %self.error,
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::RECEIVING,
        );
        counter!(
            "component_errors_total", 1,
            "endpoint" => self.endpoint.to_owned(),
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::RECEIVING,
        );
        // deprecated
        counter!("http_request_errors_total", 1);
    }
}

pub(crate) struct NginxMetricsStubStatusParseError<'a> {
    pub error: ParseError,
    pub endpoint: &'a str,
}

impl<'a> InternalEvent for NginxMetricsStubStatusParseError<'a> {
    fn emit(self) {
        error!(
            message = "NginxStubStatus parse error.",
            endpoint = %self.endpoint,
            error = %self.error,
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            "component_errors_total", 1,
            "endpoint" => self.endpoint.to_owned(),
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!("parse_errors_total", 1);
    }
}
