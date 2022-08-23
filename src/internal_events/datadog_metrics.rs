use crate::{emit, internal_events::ComponentEventsDropped};
use metrics::counter;
use vector_core::internal_event::InternalEvent;

use super::prelude::{error_stage, error_type};

#[derive(Debug)]
pub struct DatadogMetricsEncodingError {
    pub error_message: &'static str,
    pub error_code: &'static str,
    pub dropped_events: u64,
}

impl InternalEvent for DatadogMetricsEncodingError {
    fn emit(self) {
        let reason = "Failed to encode Datadog metrics.";
        error!(
            message = reason,
            error = %self.error_message,
            error_code = %self.error_code,
            error_type = error_type::ENCODER_FAILED,
            intentional = "false",
            stage = error_stage::PROCESSING,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => self.error_code,
            "error_type" => error_type::ENCODER_FAILED,
            "stage" => error_stage::PROCESSING,
        );

        if self.dropped_events > 0 {
            emit!(ComponentEventsDropped {
                count: self.dropped_events,
                intentional: false,
                reason,
            });
        }
    }
}
