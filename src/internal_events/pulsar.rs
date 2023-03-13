use metrics::counter;
use vector_core::internal_event::InternalEvent;

use crate::emit;
use vector_common::internal_event::{
    error_stage, error_type, ComponentEventsDropped, UNINTENTIONAL,
};

#[derive(Debug)]
pub struct PulsarSendingError {
    pub count: usize,
    pub error: vector_common::Error,
}

impl InternalEvent for PulsarSendingError {
    fn emit(self) {
        let reason = "A Pulsar sink generated an error.";
        error!(
            message = reason,
            error = %self.error,
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::SENDING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::SENDING,
        );
        emit!(ComponentEventsDropped::<UNINTENTIONAL> {
            count: self.count,
            reason,
        });
    }
}
