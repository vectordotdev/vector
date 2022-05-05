use metrics::counter;
use vector_core::internal_event::InternalEvent;

use crate::internal_events::prelude::{error_stage, error_type};

pub struct GcpPubsubReceiveError {
    pub error: tonic::Status,
}

impl InternalEvent for GcpPubsubReceiveError {
    fn emit(self) {
        error!(
            message = "Failed to fetch pub/sub events.",
            error = %self.error,
            error_code = "failed_fetching_events",
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::RECEIVING,
        );

        counter!(
            "component_errors_total", 1,
            "error_code" => "failed_fetching_events",
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::RECEIVING,
        );
    }
}
