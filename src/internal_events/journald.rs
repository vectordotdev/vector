use metrics::counter;
use vector_core::internal_event::InternalEvent;

use super::prelude::{error_stage, error_type};

#[derive(Debug)]
pub struct JournaldInvalidRecordError {
    pub error: serde_json::Error,
    pub text: String,
}

impl InternalEvent for JournaldInvalidRecordError {
    fn emit(self) {
        error!(
            message = "Invalid record from journald, discarding.",
            error = ?self.error,
            text = %self.text,
            stage = error_stage::PROCESSING,
            error_type = error_type::PARSER_FAILED,
        );
        counter!(
            "component_errors_total", 1,
            "stage" => error_stage::PROCESSING,
            "error_type" => error_type::PARSER_FAILED,
        );
        counter!("invalid_record_total", 1); // deprecated
        counter!("invalid_record_bytes_total", self.text.len() as u64); // deprecated
    }
}

pub struct JournaldNegativeAcknowledgmentError<'a> {
    pub cursor: &'a str,
}

impl InternalEvent for JournaldNegativeAcknowledgmentError<'_> {
    fn emit(self) {
        error!(
            message = "Event received a negative acknowledgment, journal has been stopped.",
            error_code = "negative_acknowledgement",
            error_type = error_type::ACKNOWLEDGMENT_FAILED,
            stage = error_stage::SENDING,
            cursor = self.cursor,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "negative_acknowledgment",
            "error_type" => error_type::ACKNOWLEDGMENT_FAILED,
            "stage" => error_stage::SENDING,
        );
    }
}
