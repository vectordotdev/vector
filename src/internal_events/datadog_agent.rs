use metrics::counter;
use vector_lib::NamedInternalEvent;
use vector_lib::internal_event::{InternalEvent, error_stage, error_type};

#[derive(Debug, NamedInternalEvent)]
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
        );
        counter!(
            "component_errors_total",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
    }
}
