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
            message = "Field does not exist.",
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

#[derive(Debug)]
pub struct ANSIStripperFieldInvalid<'a> {
    pub field: &'a Atom,
}

impl InternalEvent for ANSIStripperFieldInvalid<'_> {
    fn emit_logs(&self) {
        debug!(
            message = "field value must be a string.",
            field = %self.field,
            rate_limit_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!("processing_errors", 1,
            "component_kind" => "transform",
            "component_type" => "ansi_stripper",
            "error_type" => "value_invalid",
        );
    }
}

#[derive(Debug)]
pub struct ANSIStripperFailed<'a> {
    pub field: &'a Atom,
    pub error: std::io::Error,
}

impl InternalEvent for ANSIStripperFailed<'_> {
    fn emit_logs(&self) {
        debug!(
            message = "could not strip ANSI escape sequences.",
            field = %self.field,
            error = %self.error,
            rate_limit_secs = 10,
        );
    }

    fn emit_metrics(&self) {
        counter!("processing_errors", 1,
            "component_kind" => "transform",
            "component_type" => "ansi_stripper",
        );
    }
}
