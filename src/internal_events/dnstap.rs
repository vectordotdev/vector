use vector_lib::{NamedInternalEvent, counter};
use vector_lib::internal_event::{InternalEvent, MetricName, error_stage, error_type};

#[derive(Debug, NamedInternalEvent)]
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
        );
        counter!(
            MetricName::ComponentErrorsTotal,
            "stage" => error_stage::PROCESSING,
            "error_type" => error_type::PARSER_FAILED,
        )
        .increment(1);
    }
}
