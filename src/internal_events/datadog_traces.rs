use metrics::counter;
use vector_core::internal_event::InternalEvent;

use super::prelude::{error_stage, error_type};

#[derive(Debug)]
pub struct DatadogTracesEncodingError {
    pub message: &'static str,
    pub dropped_events: u64,
    pub reason: String,
}

impl InternalEvent for DatadogTracesEncodingError {
    fn emit(self) {
        error!(
            message = "Failed to encode Datadog traces.",
            error = %self.message,
            error_reason = %self.reason,
            error_type = error_type::ENCODER_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::ENCODER_FAILED,
            "stage" => error_stage::PROCESSING,
        );

        if self.dropped_events > 0 {
            counter!(
                "component_discarded_events_total", self.dropped_events,
                "error_type" => error_type::ENCODER_FAILED,
                "stage" => error_stage::PROCESSING,
            );
        }
    }
}
