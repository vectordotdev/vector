use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct ConsoleFieldNotFound {
    pub missing_field: String,
}

impl InternalEvent for ConsoleFieldNotFound {
    fn emit_logs(&self) {
        warn!(
            message = "Field not found; dropping event.",
            missing_field = ?self.missing_field,
            rate_limit_secs = 30,
        )
    }

    fn emit_metrics(&self) {
        counter!(
            "processing_errors", 1,
            "component_kind" => "sink",
            "component_type" => "console",
            "error_type" => "field_not_found",
        );
    }
}
