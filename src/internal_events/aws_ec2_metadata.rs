use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct AwsEc2MetadataEventProcessed;

impl InternalEvent for AwsEc2MetadataEventProcessed {
    fn emit_logs(&self) {
        trace!(message = "Processed one event.");
    }

    fn emit_metrics(&self) {
        counter!("processed_events_total", 1);
    }
}

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
