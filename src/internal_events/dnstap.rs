use metrics::counter;
use vector_core::internal_event::InternalEvent;

use vector_common::internal_event::{error_stage, error_type};

#[derive(Debug)]
pub(crate) struct DnstapParseError<E> {
    pub error: E,
}

impl<E: std::fmt::Display> InternalEvent for DnstapParseError<E> {
    fn emit(self) {
        error!(
            message = "Error occurred while parsing dnstap data.",
            error = %self.error,
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

#[derive(Debug)]
pub(crate) struct DnstapParseWarning<E> {
    pub error: E,
}

impl<E: std::fmt::Display> InternalEvent for DnstapParseWarning<E> {
    fn emit(self) {
        warn!(
            message = "Recoverable error occurred while parsing dnstap data.",
            error = %self.error,
            stage = error_stage::PROCESSING,
            error_type = error_type::PARSER_FAILED,
            internal_log_rate_secs = 10,
        );
    }
}
