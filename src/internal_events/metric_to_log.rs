use metrics::counter;
use serde_json::Error;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct MetricToLogFailedSerialize {
    pub error: Error,
}

impl<'a> InternalEvent for MetricToLogFailedSerialize {
    fn emit_logs(&self) {
        warn!(
            message = "Metric failed to serialize as JSON.",
            error = ?self.error,
            internal_log_rate_secs = 30
        )
    }

    fn emit_metrics(&self) {
        counter!("processing_errors_total", 1, "error_type" => "failed_serialize");
    }
}
