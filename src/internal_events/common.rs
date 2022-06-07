use std::time::Instant;

use metrics::{counter, histogram};
use vector_core::internal_event::InternalEvent;
pub use vector_core::internal_event::{EventsReceived, OldEventsReceived};

use super::prelude::{error_stage, error_type};

#[derive(Debug)]
pub struct BytesReceived<'a> {
    pub byte_size: usize,
    pub protocol: &'a str,
}

impl<'a> InternalEvent for BytesReceived<'a> {
    fn emit(self) {
        trace!(message = "Bytes received.", byte_size = %self.byte_size, protocol = %self.protocol);
        counter!("component_received_bytes_total", self.byte_size as u64, "protocol" => self.protocol.to_string());
    }
}

#[derive(Debug)]
pub struct EndpointBytesReceived<'a> {
    pub byte_size: usize,
    pub protocol: &'a str,
    pub endpoint: &'a str,
}

impl InternalEvent for EndpointBytesReceived<'_> {
    fn emit(self) {
        trace!(
            message = "Bytes received.",
            byte_size = %self.byte_size,
            protocol = %self.protocol,
            endpoint = %self.endpoint,
        );
        counter!(
            "component_received_bytes_total", self.byte_size as u64,
            "protocol" => self.protocol.to_owned(),
            "endpoint" => self.endpoint.to_owned(),
        );
    }
}

#[derive(Debug)]
pub struct EndpointBytesSent<'a> {
    pub byte_size: usize,
    pub protocol: &'a str,
    pub endpoint: &'a str,
}

impl<'a> InternalEvent for EndpointBytesSent<'a> {
    fn emit(self) {
        trace!(
            message = "Bytes sent.",
            byte_size = %self.byte_size,
            protocol = %self.protocol,
            endpoint = %self.endpoint
        );
        counter!(
            "component_sent_bytes_total", self.byte_size as u64,
            "protocol" => self.protocol.to_string(),
            "endpoint" => self.endpoint.to_string()
        );
    }
}

const STREAM_CLOSED: &str = "stream_closed";

#[derive(Debug)]
pub struct StreamClosedError {
    pub error: crate::source_sender::ClosedError,
    pub count: usize,
}

impl InternalEvent for StreamClosedError {
    fn emit(self) {
        error!(
            message = "Failed to forward event(s), downstream is closed.",
            error_code = STREAM_CLOSED,
            error_type = error_type::WRITER_FAILED,
            stage = error_stage::SENDING,
            count = %self.count,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => STREAM_CLOSED,
            "error_type" => error_type::WRITER_FAILED,
            "stage" => error_stage::SENDING,
        );
        counter!(
            "component_discarded_events_total", self.count as u64,
            "error_code" => STREAM_CLOSED,
            "error_type" => error_type::WRITER_FAILED,
            "stage" => error_stage::SENDING,
        );
    }
}

#[derive(Debug)]
pub struct FieldOverwritten<'a> {
    pub(crate) field: &'a str,
}

impl<'a> InternalEvent for FieldOverwritten<'a> {
    fn emit(self) {
        debug!(message = "Field overwritten.", field = %self.field, internal_log_rate_secs = 30);
    }
}

#[derive(Debug)]
pub struct RequestCompleted {
    pub start: Instant,
    pub end: Instant,
}

impl InternalEvent for RequestCompleted {
    fn emit(self) {
        debug!(message = "Request completed.");
        counter!("requests_completed_total", 1);
        histogram!("request_duration_seconds", self.end - self.start);
    }
}

#[derive(Debug)]
pub struct CollectionCompleted {
    pub start: Instant,
    pub end: Instant,
}

impl InternalEvent for CollectionCompleted {
    fn emit(self) {
        debug!(message = "Collection completed.");
        counter!("collect_completed_total", 1);
        histogram!("collect_duration_seconds", self.end - self.start);
    }
}
