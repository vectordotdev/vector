use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct RegexParserMatchError<'a> {
    pub value: &'a [u8],
}

impl InternalEvent for RegexParserMatchError<'_> {
    fn emit_logs(&self) {
        error!(
            message = "Regex pattern failed to match.",
            error = "Regex pattern failed to match.",
            error_type = "condition_failed",
            stage = "processing",
            field = &super::truncate_string_at(&String::from_utf8_lossy(self.value), 60)[..],
            internal_log_rate_secs = 30
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error" => "Regex pattern failed to match.",
            "error_type" => "condition_failed",
            "stage" => "processing",
        );
        // deprecated
        counter!("processing_errors_total", 1, "error_type" => "failed_match");
    }
}

#[derive(Debug)]
pub struct RegexParserMissingFieldError<'a> {
    pub field: &'a str,
}

impl InternalEvent for RegexParserMissingFieldError<'_> {
    fn emit_logs(&self) {
        error!(
            message = "Field does not exist.",
            field = %self.field,
            error = "Field does not exist.",
            error_type = "condition_failed",
            stage = "processing",
            internal_log_rate_secs = 10
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
        counter!("processing_errors_total", 1, "error_type" => "missing_field");
    }
}

#[derive(Debug)]
pub struct RegexParserTargetExistsError<'a> {
    pub target_field: &'a str,
}

impl<'a> InternalEvent for RegexParserTargetExistsError<'a> {
    fn emit_logs(&self) {
        error!(
            message = "Target field already exists.",
            error = "Target field already exists.",
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
            "target_field" => self.target_field.to_string(),
        );
        // deprecated
        counter!("processing_errors_total", 1, "error_type" => "target_field_exists");
    }
}

#[derive(Debug)]
pub struct RegexParserConversionError<'a> {
    pub name: &'a str,
    pub error: crate::types::Error,
}

impl<'a> InternalEvent for RegexParserConversionError<'a> {
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
        // deprecated
        counter!("processing_errors_total", 1, "error_type" => "type_conversion_failed");
    }
}
