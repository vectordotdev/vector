use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct StatsdEventReceived {
    pub byte_size: usize,
}

impl InternalEvent for StatsdEventReceived {
    fn emit_logs(&self) {
        trace!(message = "received packet.", byte_size = %self.byte_size);
    }

    fn emit_metrics(&self) {
        counter!(
            "events_processed", 1,
            "component_kind" => "source",
            "component_name" => "statsd",
        );
        counter!(
            "bytes_processed", self.byte_size as u64,
            "component_kind" => "source",
            "component_name" => "statsd",
        );
    }
}

#[derive(Debug)]
pub struct StatsdInvalidRecord<'a> {
    pub error: crate::sources::statsd::parser::ParseError,
    pub text: &'a str,
}

impl InternalEvent for StatsdInvalidRecord<'_> {
    fn emit_logs(&self) {
        error!(message = "invalid packet from statsd, discarding.", error = %self.error, text = %self.text);
    }

    fn emit_metrics(&self) {
        counter!(
            "invalid_record", 1,
            "component_kind" => "source",
            "component_name" => "statsd",
        );
        counter!(
            "invalid_record_bytes", self.text.len() as u64,
            "component_kind" => "source",
            "component_name" => "statsd",
        );
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
    fn new(r#type: StatsdSocketErrorType, error: T) -> Self {
        Self { r#type, error }
    }

    pub fn bind(error: T) -> Self {
        Self::new(StatsdSocketErrorType::Bind, error)
    }

    pub fn read(error: T) -> Self {
        Self::new(StatsdSocketErrorType::Read, error)
    }
}

impl<T: std::fmt::Debug + std::fmt::Display> InternalEvent for StatsdSocketError<T> {
    fn emit_logs(&self) {
        let message = match self.r#type {
            StatsdSocketErrorType::Bind => "failed to bind to UDP listener socket.",
            StatsdSocketErrorType::Read => "failed to read UDP datagram.",
        };
        error!(message, error = %self.error);
    }

    fn emit_metrics(&self) {
        counter!(
            "socket_errors", 1,
            "component_kind" => "source",
            "component_type" => "statsd",
        );
    }
}
