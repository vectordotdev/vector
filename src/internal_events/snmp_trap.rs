use metrics::counter;
use vector_lib::NamedInternalEvent;
use vector_lib::internal_event::InternalEvent;
use vector_lib::internal_event::{error_stage, error_type};

#[derive(Debug, NamedInternalEvent)]
pub struct SnmpTrapParseError {
    pub error: String,
}

impl InternalEvent for SnmpTrapParseError {
    fn emit(self) {
        error!(
            message = "Error parsing SNMP trap.",
            error = %self.error,
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
