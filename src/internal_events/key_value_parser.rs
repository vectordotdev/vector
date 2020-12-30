use super::InternalEvent;
use crate::event::LookupBuf;
use metrics::counter;

#[derive(Debug)]
pub(crate) struct KeyValueParseFailed<'a> {
    pub key: &'a LookupBuf,
    pub error: crate::types::Error,
}

impl<'a> InternalEvent for KeyValueParseFailed<'a> {
    fn emit_logs(&self) {
        warn!(
            message = "Event failed to parse as key/value.",
            key = %self.key,
            error = %self.error,
            internal_log_rate_secs = 30
        )
    }

    fn emit_metrics(&self) {
        counter!("processing_errors_total", 1,
            "error_type" => "failed_parse",
        );
    }
}

#[derive(Debug)]
pub(crate) struct KeyValueTargetExists<'a> {
    pub target_field: &'a LookupBuf,
}

impl<'a> InternalEvent for KeyValueTargetExists<'a> {
    fn emit_logs(&self) {
        warn!(
            message = "Target field already exists.",
            target_field = %self.target_field,
            internal_log_rate_secs = 30
        )
    }

    fn emit_metrics(&self) {
        counter!("processing_errors_total", 1,
            "error_type" => "target_field_exists",
        );
    }
}

#[derive(Debug)]
pub(crate) struct KeyValueFieldDoesNotExist<'a> {
    pub field: &'a LookupBuf,
}

impl<'a> InternalEvent for KeyValueFieldDoesNotExist<'a> {
    fn emit_logs(&self) {
        warn!(
            message = "Field specified does not exist.",
            field = %self.field,
            internal_log_rate_secs = 30
        )
    }

    fn emit_metrics(&self) {
        counter!("processing_errors_total", 1,
            "error_type" => "failed_parse",
        );
    }
}

#[derive(Debug)]
pub(crate) struct KeyValueMultipleSplitResults {
    pub pair: String,
}
