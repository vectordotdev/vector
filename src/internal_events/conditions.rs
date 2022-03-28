use super::prelude::{error_stage, error_type};
use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug, Copy, Clone)]
pub struct VrlConditionExecutionError<'a> {
    pub error: &'a str,
}

impl<'a> InternalEvent for VrlConditionExecutionError<'a> {
    fn emit(self) {
        error!(
            message = "VRL condition execution failed.",
            error = %self.error,
            internal_log_rate_secs = 120,
            error_type = error_type::SCRIPT_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::SCRIPT_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!("processing_errors_total", 1);
    }
}
