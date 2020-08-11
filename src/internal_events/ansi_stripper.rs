use super::InternalEvent;
use metrics::counter;
use string_cache::DefaultAtom as Atom;

#[derive(Debug)]
pub struct ANSIStripperEventProcessed;

impl InternalEvent for ANSIStripperEventProcessed {
    fn emit_metrics(&self) {
        counter!("events_processed", 1,
            "component_kind" => "transform",
            "component_type" => "ansi_stripper",
        );
    }
}

#[derive(Debug)]
pub struct ANSIStripperFieldMissing<'a> {
    pub field: &'a Atom,
}

impl InternalEvent for ANSIStripperFieldMissing<'_> {
    fn emit_logs(&self) {
        debug!(
            message = "field does not exist.",
            field = %self.field,
            rate_limit_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!("processing_errors", 1,
            "component_kind" => "transform",
            "component_type" => "ansi_stripper",
            "error_type" => "field_missing",
        );
    }
}
