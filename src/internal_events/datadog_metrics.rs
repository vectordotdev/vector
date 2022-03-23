use super::prelude::{error_stage, error_type};
use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct DatadogMetricsEncodingError {
    pub message: &'static str,
    pub error_code: &'static str,
    pub dropped_events: u64,
}

impl InternalEvent for DatadogMetricsEncodingError {
    fn emit(self) {
        error!(
            message = "Failed to encode Datadog metrics.",
            error = %self.message,
            error_code = %self.error_code,
            error_type = error_type::ENCODER_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => self.error_code,
            "error_type" => error_type::ENCODER_FAILED,
            "stage" => error_stage::PROCESSING,
        );

        if self.dropped_events > 0 {
            counter!(
                "component_discarded_events_total", self.dropped_events,
                "error_code" => self.error_code,
                "error_type" => error_type::ENCODER_FAILED,
                "stage" => error_stage::PROCESSING,
            );
        }
    }
}
