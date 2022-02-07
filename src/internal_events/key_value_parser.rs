use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct KeyValueParserError {
    pub key: String,
    pub error: crate::types::Error,
}

impl InternalEvent for KeyValueParserError {
    fn emit_logs(&self) {
        error!(
            message = "Event failed to parse as key/value.",
            key = %self.key,
            error = %self.error,
            error_type = "parser_failed",
            stage = "processing",
            internal_log_rate_secs = 30
        )
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error" => self.error.to_string(),
            "error_type" => "parser_failed",
            "stage" => "processing",
            "key" => self.key.clone(),
        );
        // deprecated
        counter!(
            "processing_errors_total", 1,
            "error_type" => "failed_parse",
        );
    }
}

#[derive(Debug)]
pub struct KeyValueTargetExistsError<'a> {
    pub target_field: &'a String,
}

impl<'a> InternalEvent for KeyValueTargetExistsError<'a> {
    fn emit_logs(&self) {
        warn!(
            message = "Target field already exists.",
            error =  "Target field already exists.",
            error_type = "condition_failed",
            stage = "processing",
            target_field = %self.target_field,
            internal_log_rate_secs = 30
        )
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error" => "Target field already exists.",
            "error_type" => "condition_failed",
            "stage" => "processing",
            "target_field" => self.target_field.clone(),
        );
        // deprecated
        counter!(
            "processing_errors_total", 1,
            "error_type" => "target_field_exists",
        );
    }
}

#[derive(Debug)]
pub struct KeyValueFieldDoesNotExistError {
    pub field: String,
}

impl InternalEvent for KeyValueFieldDoesNotExistError {
    fn emit_logs(&self) {
        error!(
            message = "Field specified does not exist.",
            error = "Field specified does not exist.",
            error_type = "condition_failed",
            stage = "processing",
            field = %self.field,
            internal_log_rate_secs = 30
        )
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error" => "Field specified does not exist.",
            "error_type" => "condition_failed",
            "stage" => "processing",
            "target_field" => self.field.clone(),
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
