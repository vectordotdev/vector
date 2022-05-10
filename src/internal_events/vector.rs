use super::prelude::{error_stage, error_type};
use metrics::counter;
use prost::DecodeError;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct VectorEventReceived {
    pub byte_size: usize,
}

impl InternalEvent for VectorEventReceived {
    fn emit(self) {
        trace!(
            message = "Events received.",
            count = 1,
            byte_size = self.byte_size
        );
        counter!("component_received_events_total", 1);
        counter!(
            "component_received_event_bytes_total",
            self.byte_size as u64
        );
        // deprecated
        counter!("events_in_total", 1);
    }
}

#[derive(Debug)]
pub struct VectorProtoDecodeError<'a> {
    pub error: &'a DecodeError,
}

impl<'a> InternalEvent for VectorProtoDecodeError<'a> {
    fn emit(self) {
        error!(
            message = "Failed to decode protobuf message.",
            error = ?self.error,
            error_code = "protobuf",
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "protobuf",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        // decoding
        counter!("protobuf_decode_errors_total", 1);
    }
}
