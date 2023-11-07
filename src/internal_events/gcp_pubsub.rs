use metrics::counter;
use vector_lib::internal_event::InternalEvent;
use vector_lib::internal_event::{error_stage, error_type};

pub struct GcpPubsubConnectError {
    pub error: tonic::transport::Error,
}

impl InternalEvent for GcpPubsubConnectError {
    fn emit(self) {
        error!(
            message = "Failed to connect to the server.",
            error = %self.error,
            error_code = "failed_connecting",
            error_type = error_type::CONNECTION_FAILED,
            stage = error_stage::RECEIVING,
            internal_log_rate_limit = true,
        );

        counter!(
            "component_errors_total", 1,
            "error_code" => "failed_connecting",
            "error_type" => error_type::CONNECTION_FAILED,
            "stage" => error_stage::RECEIVING,
        );
    }
}

pub struct GcpPubsubStreamingPullError {
    pub error: tonic::Status,
}

impl InternalEvent for GcpPubsubStreamingPullError {
    fn emit(self) {
        error!(
            message = "Failed to set up streaming pull.",
            error = %self.error,
            error_code = "failed_streaming_pull",
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::RECEIVING,
            internal_log_rate_limit = true,
        );

        counter!(
            "component_errors_total", 1,
            "error_code" => "failed_streaming_pull",
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::RECEIVING,
        );
    }
}

pub struct GcpPubsubReceiveError {
    pub error: tonic::Status,
}

impl InternalEvent for GcpPubsubReceiveError {
    fn emit(self) {
        error!(
            message = "Failed to fetch events.",
            error = %self.error,
            error_code = "failed_fetching_events",
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::RECEIVING,
            internal_log_rate_limit = true,
        );

        counter!(
            "component_errors_total", 1,
            "error_code" => "failed_fetching_events",
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::RECEIVING,
        );
    }
}
