use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct ExplodeFieldOverwritten<'a> {
    pub field: &'a str,
}

impl<'a> InternalEvent for ExplodeFieldOverwritten<'a> {
    fn emit_logs(&self) {
        debug!(message = "Field overwritten.", field = %self.field, internal_log_rate_secs = 30);
    }
}

#[derive(Debug)]
pub struct ExplodeFieldMissing<'a> {
    pub field: &'a str,
}

impl<'a> InternalEvent for ExplodeFieldMissing<'a> {
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
pub struct ExplodeFieldIsNotArray<'a> {
    pub field: &'a str,
    pub kind: &'a str,
}

impl<'a> InternalEvent for ExplodeFieldIsNotArray<'a> {
    fn emit_logs(&self) {
        warn!(
            message = "Cannot explode non array kind field.",
            field = %self.field,
            kind = %self.kind,
            internal_log_rate_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!("processing_errors_total", 1, "error_type" => "convert_failed");
    }
}
