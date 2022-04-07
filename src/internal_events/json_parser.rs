use super::prelude::{error_stage, error_type};
use metrics::counter;
use serde_json::Error;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct JsonParserError<'a> {
    pub field: &'a str,
    pub value: &'a str,
    pub error: Error,
    pub drop_invalid: bool,
}

impl<'a> InternalEvent for JsonParserError<'a> {
    fn emit(self) {
        error!(
            message = "Event failed to parse as JSON.",
            error = ?self.error,
            field = %self.field,
            value = %self.value,
            error_code = "invalid_json",
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
            drop_invalid = self.drop_invalid,
            internal_log_rate_secs = 30,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "invalid_json",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
            "field" => self.field.to_string(),
        );
        if self.drop_invalid {
            counter!(
                "component_discarded_events_total", 1,
                "error_code" => "invalid_json",
                "error_type" => error_type::PARSER_FAILED,
                "stage" => error_stage::PROCESSING,
                "field" => self.field.to_string(),
            );
        }
        // deprecated
        counter!(
            "processing_errors_total", 1,
            "error_type" => "failed_parse",
        );
    }
}
