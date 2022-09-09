use metrics::counter;

use super::{error_stage, error_type, InternalEvent};

#[derive(Debug)]
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
            "component_errors_total", 1,
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::SENDING,
        );
    }

    fn name(&self) -> Option<&'static str> {
        Some("ServicePollReadyError")
    }
}

#[derive(Debug)]
pub struct CallError<E> {
    pub error: E,
    pub request_id: usize,
}

impl<E: std::fmt::Debug> InternalEvent for CallError<E> {
    fn emit(self) {
        error!(
            message = "Service call failed.",
            request_id = self.request_id,
            error = ?self.error,
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::SENDING,
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::SENDING,
        );
    }

    fn name(&self) -> Option<&'static str> {
        Some("ServiceCallError")
    }
}
