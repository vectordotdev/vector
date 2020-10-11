use super::InternalEvent;
use metrics::counter;
use serde_json::Error;
use string_cache::DefaultAtom as Atom;

#[derive(Debug)]
pub(crate) struct JsonParserEventProcessed;

impl InternalEvent for JsonParserEventProcessed {
    fn emit_logs(&self) {
        trace!(message = "Received one event.");
    }

    fn emit_metrics(&self) {
        counter!("events_processed", 1);
    }
}

#[derive(Debug)]
pub(crate) struct JsonParserFailedParse<'a> {
    pub field: &'a Atom,
    pub value: &'a str,
    pub error: Error,
}

impl<'a> InternalEvent for JsonParserFailedParse<'a> {
    fn emit_logs(&self) {
        warn!(
            message = "Event failed to parse as JSON.",
            field = %self.field,
            value = %self.value,
            error = %self.error,
            rate_limit_secs = 30
        )
    }

    fn emit_metrics(&self) {
        counter!("processing_errors", 1,
            "error_type" => "failed_parse",
        );
    }
}

#[derive(Debug)]
pub(crate) struct JsonParserTargetExists<'a> {
    pub target_field: &'a Atom,
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
        counter!("processing_errors", 1,
            "error_type" => "target_field_exists",
        );
    }
}
