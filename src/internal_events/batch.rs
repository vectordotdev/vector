use super::prelude::{error_stage, error_type};
use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct LargeEventDroppedError {
    pub(crate) length: usize,
    pub max_length: usize,
}

impl InternalEvent for LargeEventDroppedError {
    fn emit(self) {
        error!(
            message = "Event larger than batch max_bytes; dropping event.",
            batch_max_bytes = %self.max_length,
            length = %self.length,
            internal_log_rate_secs = 1,
            error_type = error_type::CONDITION_FAILED,
            stage = error_stage::SENDING,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "oversized",
            "error_type" => error_type::CONDITION_FAILED,
            "stage" => error_stage::SENDING,
        );
        counter!(
            "component_discarded_events_total", 1,
            "error_code" => "oversized",
            "error_type" => error_type::CONDITION_FAILED,
            "stage" => error_stage::SENDING,
        );
        // deprecated
        counter!(
            "events_discarded_total", 1,
            "reason" => "oversized",
        );
    }
}
