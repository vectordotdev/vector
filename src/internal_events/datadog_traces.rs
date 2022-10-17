use crate::emit;
use metrics::counter;
use vector_core::internal_event::InternalEvent;

use vector_common::internal_event::{
    error_stage, error_type, ComponentEventsDropped, UNINTENTIONAL,
};

#[derive(Debug)]
pub struct DatadogTracesEncodingError {
    pub error_message: &'static str,
    pub error_reason: String,
    pub dropped_events: usize,
}

impl InternalEvent for DatadogTracesEncodingError {
    fn emit(self) {
        let reason = "Failed to encode Datadog traces.";
        error!(
            message = reason,
            error = %self.error_message,
            error_reason = %self.error_reason,
            error_type = error_type::ENCODER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::ENCODER_FAILED,
            "stage" => error_stage::PROCESSING,
        );

        if self.dropped_events > 0 {
            emit!(ComponentEventsDropped::<UNINTENTIONAL> {
                count: self.dropped_events,
                reason,
            });
        }
    }
}
