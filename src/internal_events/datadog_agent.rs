use metrics::counter;

use vector_lib::internal_event::InternalEvent;
use vector_lib::internal_event::{error_stage, error_type};

#[derive(Debug)]
pub struct DatadogAgentJsonParseError<'a> {
    pub error: &'a serde_json::Error,
}

impl InternalEvent for DatadogAgentJsonParseError<'_> {
    fn emit(self) {
        error!(
            message = "Failed to parse JSON body.",
            error = ?self.error,
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
    }
}
