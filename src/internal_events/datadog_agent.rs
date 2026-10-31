use vector_lib::{
    NamedInternalEvent, counter,
    internal_event::{CounterName, InternalEvent, error_stage, error_type},
};

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

#[derive(Debug, NamedInternalEvent)]
pub struct DatadogAgentUnsupportedTracePayloadError {
    pub error_code: &'static str,
}

impl InternalEvent for DatadogAgentUnsupportedTracePayloadError {
    fn emit(self) {
        error!(
            message = "Unsupported Datadog Agent trace payload shape.",
            error_code = self.error_code,
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
            "error_code" => self.error_code,
        )
        .increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct DatadogAgentInvalidIdxStringRefError {
    pub index: u32,
}

impl InternalEvent for DatadogAgentInvalidIdxStringRefError {
    fn emit(self) {
        error!(
            message = "Datadog Agent idx trace payload has an invalid string table ref.",
            error_code = "idx_invalid_string_ref",
            index = self.index,
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
            "error_code" => "idx_invalid_string_ref",
        )
        .increment(1);
    }
}
