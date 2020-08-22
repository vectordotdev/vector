use super::InternalEvent;
use metrics::counter;
use string_cache::DefaultAtom as Atom;

#[derive(Debug)]
pub struct KeyValueEventProcessed;

impl InternalEvent for KeyValueEventProcessed {
    fn emit_metrics(&self) {
        counter!("events_processed", 1,
            "component_kind" => "transform",
            "component_type" => "key_value",
        );
    }
}

#[derive(Debug)]
pub struct KeyValueFailedParse {
    pub field: Atom,
    pub error: crate::types::Error,
}

impl InternalEvent for KeyValueFailedParse {
    fn emit_logs(&self) {
        warn!(
            message = "Event failed to parse as KeyValue",
            field = %self.field,
            %self.error,
            rate_limit_secs = 30
        )
    }

    fn emit_metrics(&self) {
        counter!("processing_error", 1,
            "component_kind" => "transform",
            "component_type" => "key_value_parser",
            "error_type" => "failed_parse",
        );
    }
}
