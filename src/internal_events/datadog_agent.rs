use vector_lib::internal_event::{CounterName, InternalEvent, error_stage, error_type};
use vector_lib::{NamedInternalEvent, counter};

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
            CounterName::ComponentErrorsTotal,
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
    }
}
