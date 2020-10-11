use super::InternalEvent;
use metrics::counter;
use serde_json::Error;

#[derive(Debug)]
pub(crate) struct MetricToLogEventProcessed;

impl InternalEvent for MetricToLogEventProcessed {
    fn emit_logs(&self) {
        trace!(message = "Processed one event.");
    }

    fn emit_metrics(&self) {
        counter!("events_processed", 1);
    }
}

#[derive(Debug)]
pub(crate) struct MetricToLogFailedSerialize {
    pub error: Error,
}

impl<'a> InternalEvent for MetricToLogFailedSerialize {
    fn emit_logs(&self) {
        warn!(
            message = "Metric failed to serialize as JSON.",
            %self.error,
            rate_limit_secs = 30
        )
    }

    fn emit_metrics(&self) {
        counter!("processing_errors", 1, "error_type" => "failed_serialize");
    }
}
