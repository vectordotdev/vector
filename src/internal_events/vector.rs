use metrics::counter;
use prost::DecodeError;
use vector_core::internal_event::InternalEvent;

use super::prelude::{error_stage, error_type};

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
