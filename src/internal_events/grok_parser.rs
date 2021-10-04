use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct GrokParserFailedMatch<'a> {
    pub value: &'a str,
}

impl InternalEvent for GrokParserFailedMatch<'_> {
    fn emit_logs(&self) {
        warn!(
            message = "Grok pattern failed to match.",
            field = &super::truncate_string_at(self.value, 60)[..],
            internal_log_rate_secs = 30
        );
    }

    fn emit_metrics(&self) {
        counter!("processing_errors_total", 1,
            "error_type" => "failed_match",
        );
    }
}

#[derive(Debug)]
pub struct GrokParserMissingField<'a> {
    pub field: &'a str,
}

impl InternalEvent for GrokParserMissingField<'_> {
    fn emit_logs(&self) {
        warn!(message = "Field does not exist.", field = %self.field);
    }

    fn emit_metrics(&self) {
        counter!("processing_errors_total", 1,
            "error_type" => "missing_field",
        );
    }
}

#[derive(Debug)]
pub struct GrokParserConversionFailed<'a> {
    pub name: &'a str,
    pub error: crate::types::Error,
}

impl<'a> InternalEvent for GrokParserConversionFailed<'a> {
    fn emit_logs(&self) {
        warn!(
            message = "Could not convert types.",
            name = %self.name,
            error = ?self.error,
            internal_log_rate_secs = 30
        );
    }

    fn emit_metrics(&self) {
        counter!("processing_errors_total", 1,
            "error_type" => "type_conversion_failed",
        );
    }
}
