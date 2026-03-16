use metrics::counter;
use vector_lib::NamedInternalEvent;
use vector_lib::internal_event::{InternalEvent, error_stage, error_type};

#[derive(NamedInternalEvent)]
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
        );

        counter!(
            "component_errors_total",
            "error_code" => "failed_connecting",
            "error_type" => error_type::CONNECTION_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}

#[derive(NamedInternalEvent)]
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
        );

        counter!(
            "component_errors_total",
            "error_code" => "failed_streaming_pull",
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}

#[derive(NamedInternalEvent)]
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
        );

        counter!(
            "component_errors_total",
            "error_code" => "failed_fetching_events",
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}
