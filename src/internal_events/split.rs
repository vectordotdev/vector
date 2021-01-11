use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct SplitFieldMissing<'a> {
    pub field: &'a str,
}

impl<'a> InternalEvent for SplitFieldMissing<'a> {
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
pub struct SplitConvertFailed<'a> {
    pub field: &'a str,
    pub error: crate::types::Error,
}

impl<'a> InternalEvent for SplitConvertFailed<'a> {
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
