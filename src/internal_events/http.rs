use metrics::counter;
use std::error::Error;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct HttpEventsReceived {
    pub events_count: usize,
    pub byte_size: usize,
}

impl InternalEvent for HttpEventsReceived {
    fn emit_logs(&self) {
        trace!(
            message = "Received events.",
            events_count = %self.events_count,
            byte_size = %self.byte_size,
        );
    }

    fn emit_metrics(&self) {
        counter!("component_received_events_total", self.events_count as u64);
        counter!("events_in_total", self.events_count as u64);
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
        warn!(
            message = "Failed decompressing payload.",
            encoding= %self.encoding,
            error = %self.error,
            internal_log_rate_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!("parse_errors_total", 1);
    }
}
