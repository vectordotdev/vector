use std::error::Error;

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
    pub error_code: u16,
    pub error_message: &'a str,
}

impl<'a> InternalEvent for HttpBadRequest<'a> {
    fn emit_logs(&self) {
        warn!(
            message = "Received bad request.",
            code = ?self.error_code,
            error_message = ?self.error_message,
            internal_log_rate_secs = 10,
        );
    }

    fn emit_metrics(&self) {
        counter!("http_bad_requests_total", 1);
    }
}

#[derive(Debug)]
pub struct HttpEventMissingMessage;

impl InternalEvent for HttpEventMissingMessage {
    fn emit_logs(&self) {
        warn!(
            message = "Event missing the message key; dropping event.",
            internal_log_rate_secs = 30,
        );
    }

    fn emit_metrics(&self) {
        counter!("events_discarded_total", 1);
        counter!("component_discarded_events_total", 1);
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
            encoding= %self.encoding,
            error = %self.error,
            stage = "receiving",
            internal_log_rate_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!("parse_errors_total", 1);
        counter!(
            "component_errors_total", 1,
            "error_type" => "parse_failed",
            "stage" => "receiving",
            "encoding" => self.encoding.to_string(),
        );
    }
}
