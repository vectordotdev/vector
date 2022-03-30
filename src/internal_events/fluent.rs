// ## skip check-events ##

use metrics::counter;
use vector_core::internal_event::InternalEvent;

use crate::sources::fluent::DecodeError;

#[derive(Debug)]
pub struct FluentMessageReceived {
    pub byte_size: u64,
}

impl InternalEvent for FluentMessageReceived {
    fn emit(self) {
        trace!(message = "Received fluent message.", byte_size = %self.byte_size);
        counter!("component_received_events_total", 1);
        counter!("events_in_total", 1);
    }
}

#[derive(Debug)]
pub struct FluentMessageDecodeError<'a> {
    pub error: &'a DecodeError,
    pub base64_encoded_message: String,
}

impl<'a> InternalEvent for FluentMessageDecodeError<'a> {
    fn emit(self) {
        error!(message = "Error decoding fluent message.", error = ?self.error, base64_encoded_message = %self.base64_encoded_message, internal_log_rate_secs = 10);
        counter!("decode_errors_total", 1);
    }
}
