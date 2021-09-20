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
pub struct AwsEc2MetadataRefreshFailed {
    pub error: crate::Error,
}

impl InternalEvent for AwsEc2MetadataRefreshFailed {
    fn emit_logs(&self) {
        warn!(message = "AWS EC2 metadata refresh failed.", error = %self.error);
    }

    fn emit_metrics(&self) {
        counter!("metadata_refresh_failed_total", 1);
    }
}
