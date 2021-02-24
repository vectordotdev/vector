use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub(crate) struct TokenizerFieldMissing<'a> {
    pub field: &'a str,
}

impl<'a> InternalEvent for TokenizerFieldMissing<'a> {
    fn emit_logs(&self) {
        warn!(
            message = "Field does not exist.",
            field = %self.field,
            internal_log_rate_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!("processing_errors_total", 1, "error_type" => "field_missing");
    }
}

#[derive(Debug)]
pub(crate) struct TokenizerConvertFailed<'a> {
    pub field: &'a str,
    pub error: crate::types::Error,
}

impl<'a> InternalEvent for TokenizerConvertFailed<'a> {
    fn emit_logs(&self) {
        warn!(
            message = "Could not convert types.",
            field = %self.field,
            error = ?self.error,
            internal_log_rate_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!("processing_errors_total", 1, "error_type" => "convert_failed");
    }
}
