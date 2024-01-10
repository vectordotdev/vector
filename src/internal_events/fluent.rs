use metrics::counter;
use vector_lib::internal_event::InternalEvent;
use vector_lib::internal_event::{error_stage, error_type};

use crate::sources::fluent::DecodeError;

#[derive(Debug)]
pub struct FluentMessageReceived {
    pub byte_size: u64,
}

impl InternalEvent for FluentMessageReceived {
    fn emit(self) {
        trace!(message = "Received fluent message.", byte_size = %self.byte_size);
        counter!("component_received_events_total").increment(1);
    }
}

#[derive(Debug)]
pub struct FluentMessageDecodeError<'a> {
    pub error: &'a DecodeError,
    pub base64_encoded_message: String,
}

impl<'a> InternalEvent for FluentMessageDecodeError<'a> {
    fn emit(self) {
        error!(
            message = "Error decoding fluent message.",
            error = ?self.error,
            base64_encoded_message = %self.base64_encoded_message,
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
    }
}
