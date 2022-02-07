use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct GrokParserMatchError<'a> {
    pub value: &'a str,
}

impl InternalEvent for GrokParserMatchError<'_> {
    fn emit_logs(&self) {
        error!(
            message = "Grok pattern failed to match.",
            field = &super::truncate_string_at(self.value, 60)[..],
            error = "Grok pattern failed to match.",
            error_type = "condition_failed",
            stage = "processing",
            internal_log_rate_secs = 30
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error" => "Grok pattern failed to match.",
            "error_type" => "condition_failed",
            "stage" => "processing",
        );
        // deprecated
        counter!(
            "processing_errors_total", 1,
            "error_type" => "failed_match",
        );
    }
}

#[derive(Debug)]
pub struct GrokParserMissingFieldError<'a> {
    pub field: &'a str,
}

impl InternalEvent for GrokParserMissingFieldError<'_> {
    fn emit_logs(&self) {
        error!(
            message = "Field does not exist.",
            field = %self.field,
            error = "Field does not exist.",
            error_type = "condition_failed",
            stage = "processing",
            internal_log_rate_secs = 10,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error" => "Field does not exist.",
            "error_type" => "condition_failed",
            "stage" => "processing",
            "field" => self.field.to_string(),
        );
        // deprecated
        counter!(
            "processing_errors_total", 1,
            "error_type" => "missing_field",
        );
    }
}

#[derive(Debug)]
pub struct GrokParserConversionError<'a> {
    pub name: &'a str,
    pub error: crate::types::Error,
}

impl<'a> InternalEvent for GrokParserConversionError<'a> {
    fn emit_logs(&self) {
        error!(
            message = "Could not convert types.",
            name = %self.name,
            error = ?self.error,
            error_type = "conversion_failed",
            stage = "processing",
            internal_log_rate_secs = 30
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error" => self.error.to_string(),
            "error_type" => "conversion_failed",
            "stage" => "processing",
            "name" => self.name.to_string(),
        );
        // deprecrated
        counter!(
            "processing_errors_total", 1,
            "error_type" => "type_conversion_failed",
        );
    }
}
