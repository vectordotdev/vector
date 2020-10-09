use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct ConsoleFieldNotFound<'a> {
    pub missing_field: &'a str,
}

impl<'a> InternalEvent for ConsoleFieldNotFound<'a> {
    fn emit_logs(&self) {
        warn!(
            message = "Field not found; dropping event.",
            missing_field = ?self.missing_field,
            rate_limit_secs = 30,
        )
    }

    fn emit_metrics(&self) {
        counter!("processing_errors", 1, "error_type" => "field_not_found");
    }
}
