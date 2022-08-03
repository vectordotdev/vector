use metrics::counter;
use vector_core::internal_event::InternalEvent;

use super::prelude::{error_stage, error_type};

#[derive(Debug)]
pub struct AwsEc2MetadataRefreshSuccessful;

impl InternalEvent for AwsEc2MetadataRefreshSuccessful {
    fn emit(self) {
        debug!(message = "AWS EC2 metadata refreshed.");
        counter!("metadata_refresh_successful_total", 1);
    }
}

#[derive(Debug)]
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
            "component_errors_total", 1,
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!("metadata_refresh_failed_total", 1);
    }
}
