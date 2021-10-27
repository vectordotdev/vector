use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct DatadogMetricsEncodingError {
    pub error: &'static str,
    pub dropped_events: u64,
}

impl InternalEvent for DatadogMetricsEncodingError {
    fn emit_logs(&self) {
        error!(
            message = "Failed to encode Datadog metrics.",
            error_type = "encode_failed",
            error = %self.error,
            stage = "processing"
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error_type" => "encode_failed",
            "error" => self.error,
            "stage" => "processing",
        );

        if self.dropped_events > 0 {
            counter!("component_discarded_events_total", self.dropped_events);
        }
    }
}
