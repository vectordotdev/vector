use vector_lib::{
    NamedInternalEvent, counter,
    internal_event::{CounterName, InternalEvent, error_stage, error_type},
};

use crate::sources::fluent::DecodeError;

#[derive(Debug, NamedInternalEvent)]
pub struct FluentMessageReceived {
    pub byte_size: u64,
}

impl InternalEvent for FluentMessageReceived {
    fn emit(self) {
        trace!(message = "Received fluent message.", byte_size = %self.byte_size);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct FluentMessageDecodeError<'a> {
    pub error: &'a DecodeError,
    pub base64_encoded_message: String,
}

impl InternalEvent for FluentMessageDecodeError<'_> {
    fn emit(self) {
        error!(
            message = "Error decoding fluent message.",
            error = ?self.error,
            base64_encoded_message = %self.base64_encoded_message,
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
    }
}
