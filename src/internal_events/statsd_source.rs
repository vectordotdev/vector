// ## skip check-events ##

use bytes::Bytes;
use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct StatsdEventsReceived {
    pub byte_size: usize,
}

impl InternalEvent for StatsdEventsReceived {
    fn emit_logs(&self) {
        trace!(
            message = "Events received.",
            count = 1,
            byte_size = %self.byte_size,
        );
    }

    fn emit_metrics(&self) {
        counter!("component_received_events_total", 1);
        counter!(
            "component_received_event_bytes_total",
            self.byte_size as u64
        );
        // deprecated
        counter!("events_in_total", 1);
        counter!("processed_bytes_total", self.byte_size as u64,);
    }
}

#[derive(Debug)]
pub struct StatsdInvalidRecordError<'a> {
    pub error: &'a crate::sources::statsd::parser::ParseError,
    pub bytes: Bytes,
}

impl<'a> InternalEvent for StatsdInvalidRecordError<'a> {
    fn emit_logs(&self) {
        error!(
            message = "Invalid packet from statsd, discarding.",
            error = %self.error,
            error_type = "parse_error",
            stage = "processing",
            bytes = %String::from_utf8_lossy(&self.bytes)
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error" => self.error.to_string(),
            "error_type" => "parse_error",
            "stage" => "processing",
        );
        // deprecated
        counter!("invalid_record_total", 1,);
        counter!("invalid_record_bytes_total", self.bytes.len() as u64);
    }
}

#[derive(Debug)]
enum StatsdSocketErrorType {
    Bind,
    Read,
}

#[derive(Debug)]
pub struct StatsdSocketError<T> {
    r#type: StatsdSocketErrorType,
    pub error: T,
}

impl<T> StatsdSocketError<T> {
    const fn new(r#type: StatsdSocketErrorType, error: T) -> Self {
        Self { r#type, error }
    }

    pub const fn bind(error: T) -> Self {
        Self::new(StatsdSocketErrorType::Bind, error)
    }

    #[allow(clippy::missing_const_for_fn)] // const cannot run destructor
    pub fn read(error: T) -> Self {
        Self::new(StatsdSocketErrorType::Read, error)
    }
}

impl<T: std::fmt::Debug + std::fmt::Display> InternalEvent for StatsdSocketError<T> {
    fn emit_logs(&self) {
        let message = match self.r#type {
            StatsdSocketErrorType::Bind => "Failed to bind to UDP listener socket.",
            StatsdSocketErrorType::Read => "Failed to read UDP datagram.",
        };
        error!(
            message,
            error = %self.error,
            error_type = "connection_failed",
            stage = "processing",
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error" => self.error.to_string(),
            "error_type" => "parse_error",
            "stage" => "receiving",
        );
        // deprecated
        counter!("connection_errors_total", 1);
    }
}
