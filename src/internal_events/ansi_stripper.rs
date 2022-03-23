use super::prelude::{error_stage, error_type};
use metrics::counter;
use vector_core::internal_event::InternalEvent;

const FIELD_MISSING: &str = "field_missing";

#[derive(Debug)]
pub struct AnsiStripperFieldMissingError<'a> {
    pub field: &'a str,
}

impl InternalEvent for AnsiStripperFieldMissingError<'_> {
    fn emit(self) {
        debug!(
            message = "Field does not exist.",
            field = %self.field,
            error_code = FIELD_MISSING,
            error_type = error_type::CONDITION_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_secs = 10
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => FIELD_MISSING,
            "error_type" => error_type::CONDITION_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!("processing_errors_total", 1, "error_type" => "field_missing");
    }
}

const EXPECTED_STRING: &str = "expected_string";

#[derive(Debug)]
pub struct AnsiStripperFieldInvalidError<'a> {
    pub field: &'a str,
}

impl InternalEvent for AnsiStripperFieldInvalidError<'_> {
    fn emit(self) {
        error!(
            message = "Field value must be a string.",
            field = %self.field,
            error_code = EXPECTED_STRING,
            error_type = error_type::CONDITION_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_secs = 10,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => EXPECTED_STRING,
            "error_type" => error_type::CONDITION_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!("processing_errors_total", 1, "error_type" => "value_invalid");
    }
}

const COULDNT_STRIP: &str = "could_not_strip";

#[derive(Debug)]
pub struct AnsiStripperError<'a> {
    pub field: &'a str,
    pub error: std::io::Error,
}

impl InternalEvent for AnsiStripperError<'_> {
    fn emit(self) {
        error!(
            message = "Could not strip ANSI escape sequences.",
            field = %self.field,
            error = ?self.error,
            error_code = COULDNT_STRIP,
            error_type = error_type::CONVERSION_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_secs = 10,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => COULDNT_STRIP,
            "error_type" => error_type::CONVERSION_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!("processing_errors_total", 1);
    }
}
