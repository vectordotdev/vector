use vector_lib::{
    NamedInternalEvent, counter,
    internal_event::{CounterName, InternalEvent, error_stage, error_type},
};
use vrl::{path::PathParseError, value::KeyString};

#[derive(Debug, NamedInternalEvent)]
pub struct ReduceStaleEventFlushed;

impl InternalEvent for ReduceStaleEventFlushed {
    fn emit(self) {
        counter!(CounterName::StaleEventsFlushedTotal).increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
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
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
    }
}
