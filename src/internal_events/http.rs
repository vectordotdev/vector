use std::error::Error;

use super::prelude::{error_stage, error_type, http_error_code};
use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct HttpBytesReceived<'a> {
    pub byte_size: usize,
    pub http_path: &'a str,
    pub protocol: &'static str,
}

impl InternalEvent for HttpBytesReceived<'_> {
    fn emit_logs(&self) {
        trace!(
            message = "Received bytes.",
            byte_size = %self.byte_size,
            http_path = %self.http_path,
            protocol = %self.protocol
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_received_bytes_total", self.byte_size as u64,
            "http_path" => self.http_path.to_string(),
            "protocol" => self.protocol,
        );
    }
}

#[derive(Debug)]
pub struct HttpEventsReceived<'a> {
    pub count: usize,
    pub byte_size: usize,
    pub http_path: &'a str,
    pub protocol: &'static str,
}

impl InternalEvent for HttpEventsReceived<'_> {
    fn emit_logs(&self) {
        trace!(
            message = "Received events.",
            count = %self.count,
            byte_size = %self.byte_size,
            http_path = %self.http_path,
            protocol = %self.protocol,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_received_events_total", self.count as u64,
            "http_path" => self.http_path.to_string(),
            "protocol" => self.protocol,
        );
        counter!(
            "component_received_event_bytes_total",
            self.byte_size as u64,
            "http_path" => self.http_path.to_string(),
            "protocol" => self.protocol,
        );
        counter!("events_in_total", self.count as u64);
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct HttpBadRequest<'a> {
    pub code: u16,
    pub message: &'a str,
}

impl<'a> HttpBadRequest<'a> {
    fn error_code(&self) -> String {
        http_error_code(self.code)
    }
}

impl<'a> InternalEvent for HttpBadRequest<'a> {
    fn emit_logs(&self) {
        warn!(
            message = "Received bad request.",
            error = %self.message,
            error_code = %self.error_code(),
            error_type = error_type::REQUEST_FAILED,
            error_stage = error_stage::RECEIVING,
            http_code = %self.code,
            internal_log_rate_secs = 10,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error_code" => self.error_code(),
            "error_type" => error_type::REQUEST_FAILED,
            "error_stage" => error_stage::RECEIVING,
        );
        // deprecated
        counter!("http_bad_requests_total", 1);
    }
}

#[derive(Debug)]
pub struct HttpEventEncoded {
    pub byte_size: usize,
}

impl InternalEvent for HttpEventEncoded {
    fn emit_logs(&self) {
        trace!(message = "Encode event.");
    }

    fn emit_metrics(&self) {
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct HttpDecompressError<'a> {
    pub error: &'a dyn Error,
    pub encoding: &'a str,
}

impl<'a> InternalEvent for HttpDecompressError<'a> {
    fn emit_logs(&self) {
        error!(
            message = "Failed decompressing payload.",
            error = %self.error,
            error_code = "failed_decompressing_payload",
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::RECEIVING,
            encoding = %self.encoding,
            internal_log_rate_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error_code" => "failed_decompressing_payload",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::RECEIVING,
        );
        // deprecated
        counter!("parse_errors_total", 1);
    }
}
