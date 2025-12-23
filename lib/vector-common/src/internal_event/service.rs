use metrics::counter;

use super::{ComponentEventsDropped, InternalEvent, UNINTENTIONAL, emit, error_stage, error_type};
use crate::NamedInternalEvent;

#[derive(Debug, NamedInternalEvent)]
pub struct PollReadyError<E> {
    pub error: E,
}

impl<E: std::fmt::Debug> InternalEvent for PollReadyError<E> {
    fn emit(self) {
        error!(
            message = "Service poll ready failed.",
            error = ?self.error,
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::SENDING,
        );
        counter!(
            "component_errors_total",
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::SENDING,
        )
        .increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct CallError<E> {
    pub error: E,
    pub request_id: usize,
    pub count: usize,
}

impl<E: std::fmt::Debug> InternalEvent for CallError<E> {
    fn emit(self) {
        let reason = "Service call failed. No retries or retries exhausted.";
        error!(
            message = reason,
            error = ?self.error,
            request_id = self.request_id,
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::SENDING,
        );
        counter!(
            "component_errors_total",
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::SENDING,
        )
        .increment(1);

        emit(ComponentEventsDropped::<UNINTENTIONAL> {
            reason,
            count: self.count,
        });
    }
}
