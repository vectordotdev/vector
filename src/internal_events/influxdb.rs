use crate::emit;
use metrics::counter;
use vector_common::internal_event::{
    error_stage, error_type, ComponentEventsDropped, UNINTENTIONAL,
};
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct InfluxdbEncodingError {
    pub error_message: &'static str,
    pub count: usize,
}

impl InternalEvent for InfluxdbEncodingError {
    fn emit(self) {
        let reason = "Failed to encode event.";
        error!(
            message = reason,
            error = %self.error_message,
            error_type = error_type::ENCODER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::ENCODER_FAILED,
            "stage" => error_stage::PROCESSING,
        );

        emit!(ComponentEventsDropped::<UNINTENTIONAL> {
            count: self.count,
            reason
        });
    }
}
