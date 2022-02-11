use super::prelude::{error_stage, error_type};
use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct AnsiStripperFieldMissingError<'a> {
    pub field: &'a str,
}

impl InternalEvent for AnsiStripperFieldMissingError<'_> {
    fn emit_logs(&self) {
        debug!(
            message = "Field does not exist.",
            field = %self.field,
            error = "field_missing",
            error_type = error_type::CONDITION_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error" => "field_missing",
            "error_type" => error_type::CONDITION_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!("processing_errors_total", 1, "error_type" => "field_missing");
    }
}

#[derive(Debug)]
pub struct AnsiStripperFieldInvalidError<'a> {
    pub field: &'a str,
}

impl InternalEvent for AnsiStripperFieldInvalidError<'_> {
    fn emit_logs(&self) {
        error!(
            message = "Field value must be a string.",
            field = %self.field,
            error = "expected_string",
            error_type = error_type::CONDITION_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_secs = 10,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error" => "expected_string",
            "error_type" => error_type::CONDITION_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!("processing_errors_total", 1, "error_type" => "value_invalid");
    }
}

#[derive(Debug)]
pub struct AnsiStripperError<'a> {
    pub field: &'a str,
    pub error: std::io::Error,
}

impl InternalEvent for AnsiStripperError<'_> {
    fn emit_logs(&self) {
        error!(
            message = "Could not strip ANSI escape sequences.",
            field = %self.field,
            error = ?self.error,
            error_type = error_type::CONVERSION_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_secs = 10,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error" => self.error.to_string(),
            "error_type" => error_type::CONVERSION_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!("processing_errors_total", 1);
    }
}
