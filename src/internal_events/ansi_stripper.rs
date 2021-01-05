use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct ANSIStripperFieldMissing<'a> {
    pub field: &'a str,
}

impl InternalEvent for ANSIStripperFieldMissing<'_> {
    fn emit_logs(&self) {
        debug!(
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
pub struct ANSIStripperFieldInvalid<'a> {
    pub field: &'a str,
}

impl InternalEvent for ANSIStripperFieldInvalid<'_> {
    fn emit_logs(&self) {
        debug!(
            message = "Field value must be a string.",
            field = %self.field,
            internal_log_rate_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!("processing_errors_total", 1, "error_type" => "value_invalid");
    }
}

#[derive(Debug)]
pub struct ANSIStripperFailed<'a> {
    pub field: &'a str,
    pub error: std::io::Error,
}

impl InternalEvent for ANSIStripperFailed<'_> {
    fn emit_logs(&self) {
        debug!(
            message = "Could not strip ANSI escape sequences.",
            field = %self.field,
            error = ?self.error,
            internal_log_rate_secs = 10,
        );
    }

    fn emit_metrics(&self) {
        counter!("processing_errors_total", 1);
    }
}
