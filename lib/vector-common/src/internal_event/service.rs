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
            internal_log_rate_limit = true,
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
