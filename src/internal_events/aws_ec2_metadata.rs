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
            error_type = "request_failed",
            stage = "processing",
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error" => self.error.to_string(),
            "error_type" => "request_failed",
            "stage" => "processing",
        );
        // deprecated
        counter!("metadata_refresh_failed_total", 1);
    }
}
