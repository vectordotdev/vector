use metrics::counter;
use vector_lib::NamedInternalEvent;
use vector_lib::internal_event::{InternalEvent, error_stage, error_type};

#[derive(Debug, NamedInternalEvent)]
pub struct AwsEc2MetadataRefreshSuccessful;

impl InternalEvent for AwsEc2MetadataRefreshSuccessful {
    fn emit(self) {
        debug!(message = "AWS EC2 metadata refreshed.");
        counter!("metadata_refresh_successful_total").increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct AwsEc2MetadataRefreshError {
    pub error: crate::Error,
}

impl InternalEvent for AwsEc2MetadataRefreshError {
    fn emit(self) {
        error!(
            message = "AWS EC2 metadata refresh failed.",
            error = %self.error,
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            "component_errors_total",
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
        // deprecated
        counter!("metadata_refresh_failed_total").increment(1);
    }
}
