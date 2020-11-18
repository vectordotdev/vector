use super::InternalEvent;
use crate::event::Lookup;
use metrics::counter;

#[derive(Debug)]
pub struct LogfmtParserEventProcessed;

impl InternalEvent for LogfmtParserEventProcessed {
    fn emit_logs(&self) {
        trace!(message = "Processed one event.");
    }

    fn emit_metrics(&self) {
        counter!("events_processed", 1);
    }
}

#[derive(Debug)]
pub struct LogfmtParserMissingField<'a> {
    pub field: Lookup<'a>,
}

impl InternalEvent for LogfmtParserMissingField<'_> {
    fn emit_logs(&self) {
        debug!(message = "Field does not exist.", field = %self.field);
    }

    fn emit_metrics(&self) {
        counter!("processing_errors", 1,
            "error_type" => "missing_field",
        );
    }
}

#[derive(Debug)]
pub struct LogfmtParserConversionFailed<'a> {
    pub name: Lookup<'a>,
    pub error: crate::types::Error,
}

impl<'a> InternalEvent for LogfmtParserConversionFailed<'a> {
    fn emit_logs(&self) {
        debug!(
            message = "Could not convert types.",
            name = %self.name,
            error = ?self.error,
            rate_limit_secs = 30
        );
    }

    fn emit_metrics(&self) {
        counter!("processing_errors", 1,
            "error_type" => "type_conversion_failed",
        );
    }
}
