use super::InternalEvent;
use metrics::counter;
use serde_json::Error;

#[derive(Debug)]
pub(crate) struct JsonParserEventProcessed;

impl InternalEvent for JsonParserEventProcessed {
    fn emit_logs(&self) {
        trace!(message = "Received one event.");
    }

    fn emit_metrics(&self) {
        counter!("processed_events_total", 1);
    }
}

#[derive(Debug)]
pub(crate) struct JsonParserFailedParse<'a> {
    pub field: &'a str,
    pub value: &'a str,
    pub error: Error,
}

impl<'a> InternalEvent for JsonParserFailedParse<'a> {
    fn emit_logs(&self) {
        warn!(
            message = "Event failed to parse as JSON.",
            field = %self.field,
            value = %self.value,
            error = ?self.error,
            rate_limit_secs = 30
        )
    }

    fn emit_metrics(&self) {
        counter!("processing_errors_total", 1,
            "error_type" => "failed_parse",
        );
    }
}

#[derive(Debug)]
pub(crate) struct JsonParserTargetExists<'a> {
    pub target_field: &'a str,
}

impl<'a> InternalEvent for JsonParserTargetExists<'a> {
    fn emit_logs(&self) {
        warn!(
            message = "Target field already exists.",
            target_field = %self.target_field,
            rate_limit_secs = 30
        )
    }

    fn emit_metrics(&self) {
        counter!("processing_errors_total", 1,
            "error_type" => "target_field_exists",
        );
    }
}
