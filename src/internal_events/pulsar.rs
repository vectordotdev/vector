use metrics::counter;
use vector_core::internal_event::InternalEvent;

use super::prelude::{error_stage, error_type};

#[derive(Debug)]
pub struct PulsarSendingError {
    pub count: usize,
    pub error: vector_core::Error,
}

impl InternalEvent for PulsarSendingError {
    fn emit(self) {
        error!(
            message = "Events dropped.",
            reason = "A Pulsar sink generated an error.",
            error = %self.error,
            error_type = error_type::REQUEST_FAILED,
            intentional = false,
            stage = error_stage::SENDING,
            count = %self.count,
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::SENDING,
        );
        counter!(
            "component_discarded_events_total", self.count as u64,
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::SENDING,
            "intentional" => "false",
        );
    }
}
