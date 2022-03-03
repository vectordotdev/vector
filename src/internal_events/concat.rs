use super::prelude::{error_stage, error_type};
use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct ConcatSubstringError<'a> {
    pub source: &'a str,
    pub condition: &'a str,
    pub start: usize,
    pub end: usize,
    pub length: usize,
}

impl<'a> InternalEvent for ConcatSubstringError<'a> {
    fn emit_logs(&self) {
        error!(
            message = "Substring error.",
            error = "Unable to split string.",
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
            condition = self.condition,
            source = self.source,
            start = self.start,
            end = self.end,
            length = self.length,
            internal_log_rate_secs = 30,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error" => "Substring error.",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!("processing_errors_total", 1);
    }
}

#[derive(Debug)]
pub struct ConcatSubstringSourceMissing<'a> {
    pub source: &'a str,
}

impl<'a> InternalEvent for ConcatSubstringSourceMissing<'a> {
    fn emit_logs(&self) {
        debug!(
            message = "Substring source missing.",
            self.source,
            internal_log_rate_secs = 30
        );
    }
}
