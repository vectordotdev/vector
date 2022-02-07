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
    fn emit_logs(&self) {
        error!(
            message = "Event failed to parse as JSON.",
            field = %self.field,
            value = %self.value,
            error = ?self.error,
            error_type = "parser_failed",
            stage = "processing",
            drop_invalid = self.drop_invalid,
            internal_log_rate_secs = 30,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error" => self.error.to_string(),
            "error_type" => "parser_failed",
            "stage" => "processing",
            "field" => self.field.to_string(),
            "value" => self.value.to_string(),
            "drop_invalid" => self.drop_invalid.to_string(),
        );
        // deprecated
        counter!(
            "processing_errors_total", 1,
            "error_type" => "failed_parse",
        );
    }
}
