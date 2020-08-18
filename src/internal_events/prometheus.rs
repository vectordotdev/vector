use super::InternalEvent;
use crate::sources::prometheus::parser::ParserError;
use metrics::{counter, timing};
use std::time::Instant;

#[derive(Debug)]
pub struct PrometheusEventReceived {
    pub byte_size: usize,
    pub count: usize,
}

impl InternalEvent for PrometheusEventReceived {
    fn emit_logs(&self) {
        debug!(message = "Scraped events.", ?self.count);
    }

    fn emit_metrics(&self) {
        counter!(
            "events_processed", self.count as u64,
            "component_kind" => "source",
            "component_type" => "prometheus",
        );
        counter!(
            "bytes_processed", self.byte_size as u64,
            "component_kind" => "source",
            "component_type" => "prometheus",
        );
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
        counter!("requests_completed", 1,
            "component_kind" => "source",
            "component_type" => "prometheus",
        );
        timing!("request_duration_nanoseconds", self.start,self.end,
            "component_kind" => "source",
            "component_type" => "prometheus",
        );
    }
}

#[derive(Debug)]
pub struct PrometheusParseError {
    pub error: ParserError,
}

impl InternalEvent for PrometheusParseError {
    fn emit_logs(&self) {
        error!(message = "parsing error.", error = ?self.error);
    }

    fn emit_metrics(&self) {
        counter!("parse_errors", 1,
            "component_kind" => "source",
            "component_type" => "prometheus",
        );
    }
}

#[derive(Debug)]
pub struct PrometheusHttpError {
    pub error: hyper::Error,
}

impl InternalEvent for PrometheusHttpError {
    fn emit_logs(&self) {
        error!(message = "http request processing error.", error = %self.error);
    }

    fn emit_metrics(&self) {
        counter!("http_request_errors", 1,
            "component_kind" => "source",
            "component_type" => "prometheus",
        );
    }
}
