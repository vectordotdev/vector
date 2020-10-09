use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct AwsEc2MetadataEventProcessed;

impl InternalEvent for AwsEc2MetadataEventProcessed {
    fn emit_logs(&self) {
        trace!(message = "Processed one event.");
    }

    fn emit_metrics(&self) {
        counter!("events_processed", 1);
    }
}

#[derive(Debug)]
pub struct AwsEc2MetadataRefreshComplete;

impl InternalEvent for AwsEc2MetadataRefreshComplete {
    fn emit_logs(&self) {
        debug!(message = "AWS EC2 metadata refreshed.");
    }

    fn emit_metrics(&self) {
        counter!("metadata_refresh_complete", 1);
    }
}

#[derive(Debug)]
pub struct AwsEc2MetadataRequestFailed<'a> {
    pub path: &'a str,
    pub error: crate::Error,
}

impl<'a> InternalEvent for AwsEc2MetadataRequestFailed<'a> {
    fn emit_logs(&self) {
        warn!(message = "AWS EC2 metadata request failed.", %self.error, %self.path);
    }

    fn emit_metrics(&self) {
        counter!("metadata_refresh_request_failed", 1);
    }
}
