use metrics::counter;
use vector_core::internal_event::InternalEvent;

use super::prelude::{error_stage, error_type};

#[derive(Debug)]
pub(crate) struct DnstapParseError<'a> {
    pub error: &'a str,
}

impl<'a> InternalEvent for DnstapParseError<'a> {
    fn emit(self) {
        error!(
            message = "Error occurred while parsing dnstap data.",
            error = ?self.error,
            stage = error_stage::PROCESSING,
            error_type = error_type::PARSER_FAILED,
            internal_log_rate_secs = 10,
        );
        counter!(
            "component_errors_total", 1,
            "stage" => error_stage::PROCESSING,
            "error_type" => error_type::PARSER_FAILED,
        );
        // deprecated
        counter!("parse_errors_total", 1);
    }
}
