use vector_lib::{NamedInternalEvent, counter};
use vector_lib::internal_event::{InternalEvent, MetricName, error_stage, error_type};

#[derive(Debug, Copy, Clone, NamedInternalEvent)]
pub struct VrlConditionExecutionError<'a> {
    pub error: &'a str,
}

impl InternalEvent for VrlConditionExecutionError<'_> {
    fn emit(self) {
        error!(
            message = "VRL condition execution failed.",
            error = %self.error,
            error_type = error_type::SCRIPT_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            MetricName::ComponentErrorsTotal,
            "error_type" => error_type::SCRIPT_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
    }
}
