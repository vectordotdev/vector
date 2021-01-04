use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct LogfmtParserMissingField<'a> {
    pub field: &'a str,
}

impl InternalEvent for LogfmtParserMissingField<'_> {
    fn emit_logs(&self) {
        debug!(message = "Field does not exist.", field = %self.field);
    }

    fn emit_metrics(&self) {
        counter!("processing_errors_total", 1,
            "error_type" => "missing_field",
        );
    }
}

#[derive(Debug)]
pub struct LogfmtParserConversionFailed<'a> {
    pub name: &'a str,
    pub error: crate::types::Error,
}

impl<'a> InternalEvent for LogfmtParserConversionFailed<'a> {
    fn emit_logs(&self) {
        debug!(
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
