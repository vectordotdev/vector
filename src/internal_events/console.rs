use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct ConsoleFieldNotFound<'a> {
    pub missing_field: &'a str,
}

impl<'a> InternalEvent for ConsoleFieldNotFound<'a> {
    fn emit(self) {
        warn!(
            message = "Field not found; dropping event.",
            missing_field = ?self.missing_field,
            internal_log_rate_secs = 30,
        );
        counter!("processing_errors_total", 1, "error_type" => "field_not_found");
    }
}
