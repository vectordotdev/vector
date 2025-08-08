use metrics::counter;
use vector_lib::internal_event::{error_stage, error_type, InternalEvent};
use vrl::path::PathParseError;
use vrl::value::KeyString;

#[derive(Debug)]
pub struct ReduceStaleEventFlushed;

impl InternalEvent for ReduceStaleEventFlushed {
    fn emit(self) {
        counter!("stale_events_flushed_total").increment(1);
    }
}

#[derive(Debug)]
pub struct ReduceAddEventError {
    pub error: PathParseError,
    pub path: KeyString,
}

impl InternalEvent for ReduceAddEventError {
    fn emit(self) {
        error!(
            message = "Event field could not be reduced.",
            path = ?self.path,
            error = ?self.error,
            error_type = error_type::CONDITION_FAILED,
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
