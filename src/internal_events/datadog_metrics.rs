// ## skip check-events ##
use super::prelude::{error_stage, error_type};
use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct DatadogMetricsEncodingError {
    pub error: &'static str,
    pub dropped_events: u64,
}

impl InternalEvent for DatadogMetricsEncodingError {
    fn emit_logs(&self) {
        error!(
            message = "Failed to encode Datadog metrics.",
            error_type = error_type::ENCODER_FAILED,
            error = %self.error,
            stage = error_stage::PROCESSING,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::ENCODER_FAILED,
            "error" => self.error,
            "stage" => error_stage::PROCESSING,
        );

        if self.dropped_events > 0 {
            counter!("component_discarded_events_total", self.dropped_events);
        }
    }
}
