use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct JsonEventProcessed;

impl InternalEvent for JsonEventProcessed {
    fn emit_metrics(&self) {
        counter!("events_processed", 1,
            "component_kind" => "transform",
            "component_type" => "json_parser",
        );
    }
}

#[derive(Debug)]
pub struct JsonFailedParse<'a> {
    pub field: &'a Atom,
    pub error: Error,
}

impl InternalEvent for JsonFailedParse {
    fn emit_logs(&self) {
        warn!(
            message = "Event failed to parse as JSON",
            field = %self.field(),
            %error,
            rate_limit_secs = 30
        )
    }

    fn emit_metrics(&self) {
        counter!("processing_error", 1,
            "component_kind" => "transform",
            "component_type" => "json_parser",
            "error_type" => "failed_parse",
        );
    }
}
