use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct CoercerEventProcessed;

impl InternalEvent for CoercerEventProcessed {
    fn emit_metrics(&self) {
        counter!("processed_events_total", 1);
    }
}

#[derive(Debug)]
pub(crate) struct CoercerConversionFailed<'a> {
    pub field: &'a str,
    pub error: crate::types::Error,
}

impl<'a> InternalEvent for CoercerConversionFailed<'a> {
    fn emit_logs(&self) {
        error!(
            message = "Could not convert types.",
            field = %self.field,
            error = %self.error,
            rate_limit_secs = 30
        );
    }

    fn emit_metrics(&self) {
        counter!("processing_errors_total", 1, "error_type" => "type_conversion_failed");
    }
}
