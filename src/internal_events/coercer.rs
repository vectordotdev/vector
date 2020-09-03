use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct CoercerEventProcessed;

impl InternalEvent for CoercerEventProcessed {
    fn emit_metrics(&self) {
        counter!("events_processed", 1,
            "component_kind" => "transform",
            "component_type" => "coercer",
        );
    }
}

#[derive(Debug)]
pub(crate) struct CoercerConversionFailed<'a> {
    pub field: &'a str,
    pub error: crate::types::Error,
}

impl<'a> InternalEvent for CoercerConversionFailed<'a> {
    fn emit_logs(&self) {
        debug!(
            message = "Could not convert types.",
            field = %self.field,
            error = %self.error,
            rate_limit_secs = 30
        );
    }

    fn emit_metrics(&self) {
        counter!("processing_error", 1,
            "component_kind" => "transform",
            "component_type" => "coercer",
            "error_type" => "type_conversion_failed",
        );
    }
}
