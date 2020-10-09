use super::InternalEvent;
use metrics::counter;
use string_cache::DefaultAtom as Atom;

#[derive(Debug)]
pub(crate) struct TokenizerEventProcessed;

impl InternalEvent for TokenizerEventProcessed {
    fn emit_metrics(&self) {
        counter!("events_processed", 1);
    }
}

#[derive(Debug)]
pub(crate) struct TokenizerFieldMissing<'a> {
    pub field: &'a Atom,
}

impl<'a> InternalEvent for TokenizerFieldMissing<'a> {
    fn emit_logs(&self) {
        debug!(
            message = "Field does not exist.",
            field = %self.field,
            rate_limit_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!("processing_errors", 1, "error_type" => "field_missing");
    }
}

#[derive(Debug)]
pub(crate) struct TokenizerConvertFailed<'a> {
    pub field: &'a Atom,
    pub error: crate::types::Error,
}

impl<'a> InternalEvent for TokenizerConvertFailed<'a> {
    fn emit_logs(&self) {
        debug!(
            message = "Could not convert types.",
            field = %self.field,
            error = %self.error,
            rate_limit_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!("processing_errors", 1, "error_type" => "convert_failed");
    }
}
