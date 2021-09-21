use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct RegexParserFailedMatch<'a> {
    pub value: &'a [u8],
}

impl InternalEvent for RegexParserFailedMatch<'_> {
    fn emit_logs(&self) {
        warn!(
            message = "Regex pattern failed to match.",
            field = &super::truncate_string_at(&String::from_utf8_lossy(self.value), 60)[..],
            internal_log_rate_secs = 30
        );
    }

    fn emit_metrics(&self) {
        counter!("processing_errors_total", 1, "error_type" => "failed_match");
    }
}

#[derive(Debug)]
pub struct RegexParserMissingField<'a> {
    pub field: &'a str,
}

impl InternalEvent for RegexParserMissingField<'_> {
    fn emit_logs(&self) {
        warn!(message = "Field does not exist.", field = %self.field);
    }

    fn emit_metrics(&self) {
        counter!("processing_errors_total", 1, "error_type" => "missing_field");
    }
}

#[derive(Debug)]
pub struct RegexParserTargetExists<'a> {
    pub target_field: &'a str,
}

impl<'a> InternalEvent for RegexParserTargetExists<'a> {
    fn emit_logs(&self) {
        warn!(
            message = "Target field already exists.",
            target_field = %self.target_field,
            internal_log_rate_secs = 30
        )
    }

    fn emit_metrics(&self) {
        counter!("processing_errors_total", 1, "error_type" => "target_field_exists");
    }
}

#[derive(Debug)]
pub struct RegexParserConversionFailed<'a> {
    pub name: &'a str,
    pub error: crate::types::Error,
}

impl<'a> InternalEvent for RegexParserConversionFailed<'a> {
    fn emit_logs(&self) {
        debug!(
            message = "Could not convert types.",
            name = %self.name,
            error = ?self.error,
            internal_log_rate_secs = 30
        );
    }

    fn emit_metrics(&self) {
        counter!("processing_errors_total", 1, "error_type" => "type_conversion_failed");
    }
}
