use super::prelude::{error_stage, error_type};
use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct AwsEc2MetadataRefreshSuccessful;

impl InternalEvent for AwsEc2MetadataRefreshSuccessful {
    fn emit_logs(&self) {
        debug!(message = "AWS EC2 metadata refreshed.");
    }

    fn emit_metrics(&self) {
        counter!("metadata_refresh_successful_total", 1);
    }
}

#[derive(Debug)]
pub struct AwsEc2MetadataRefreshError {
    pub error: crate::Error,
}

impl InternalEvent for AwsEc2MetadataRefreshError {
    fn emit_logs(&self) {
        error!(
            message = "AWS EC2 metadata refresh failed.",
            error = %self.error,
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::PROCESSING,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error" => self.error.to_string(),
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!("metadata_refresh_failed_total", 1);
    }
}
