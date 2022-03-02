use super::prelude::{error_stage, error_type};
use bytes::Bytes;
use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct StatsdInvalidRecordError<'a> {
    pub error: &'a crate::sources::statsd::parser::ParseError,
    pub bytes: Bytes,
}

const INVALID_PACKET: &str = "invalid_packet";

impl<'a> InternalEvent for StatsdInvalidRecordError<'a> {
    fn emit_logs(&self) {
        error!(
            message = "Invalid packet from statsd, discarding.",
            error = %self.error,
            error_code = INVALID_PACKET,
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
            bytes = %String::from_utf8_lossy(&self.bytes),
            rate_limit_secs = 10,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error_code" => INVALID_PACKET,
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
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

    const fn error_code(&self) -> &'static str {
        match self.r#type {
            StatsdSocketErrorType::Bind => "failed_udp_binding",
            StatsdSocketErrorType::Read => "failed_udp_datagram",
        }
    }
}

impl<T: std::fmt::Debug + std::fmt::Display> InternalEvent for StatsdSocketError<T> {
    fn emit_logs(&self) {
        let message = match self.r#type {
            StatsdSocketErrorType::Bind => {
                format!("Failed to bind to UDP listener socket: {:?}", self.error)
            }
            StatsdSocketErrorType::Read => format!("Failed to read UDP datagram: {:?}", self.error),
        };
        let error = self.error_code();
        error!(
            message = %message,
            error = %error,
            error_code = %self.error_code(),
            error_type = error_type::CONNECTION_FAILED,
            stage = error_stage::RECEIVING,
            rate_limit_secs = 10,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error_code" => self.error_code(),
            "error_type" => error_type::CONNECTION_FAILED,
            "stage" => error_stage::RECEIVING,
        );
        // deprecated
        counter!("connection_errors_total", 1);
    }
}
