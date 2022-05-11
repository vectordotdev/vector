use metrics::counter;
use vector_core::internal_event::InternalEvent;

use super::prelude::{error_stage, error_type};

#[derive(Debug)]
pub struct KeyValueParserError {
    pub key: String,
    pub error: crate::types::Error,
}

impl InternalEvent for KeyValueParserError {
    fn emit(self) {
        error!(
            message = "Event failed to parse as key/value.",
            key = %self.key,
            error = %self.error,
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_secs = 30
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
            "key" => self.key,
        );
        // deprecated
        counter!(
            "processing_errors_total", 1,
            "error_type" => "failed_parse",
        );
    }
}

#[derive(Debug)]
pub struct KeyValueMultipleSplitResults {
    pub pair: String,
}
