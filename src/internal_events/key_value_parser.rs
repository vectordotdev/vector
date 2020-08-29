use super::InternalEvent;
use metrics::counter;
use string_cache::DefaultAtom as Atom;

#[derive(Debug)]
pub(crate) struct KeyValueEventProcessed;

impl InternalEvent for KeyValueEventProcessed {
    fn emit_metrics(&self) {
        counter!("events_processed", 1,
            "component_kind" => "transform",
            "component_type" => "key_value",
        );
    }
}

#[derive(Debug)]
pub(crate) struct KeyFailedParse {
    pub key: Atom,
    pub error: crate::types::Error,
}

impl InternalEvent for KeyFailedParse {
    fn emit_logs(&self) {
        warn!(
            message = "Event failed to parse as KeyValue",
            key = %self.key,
            error = %self.error,
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

#[derive(Debug)]
pub(crate) struct KeyValueEventFailed {
    pub field: Atom,
    pub error: Atom,
}

impl InternalEvent for KeyValueEventFailed {
    fn emit_logs(&self) {
        warn!(
            message = "Event failed to parse as KeyValue",
            field = %self.field,
            error = %self.error,
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
