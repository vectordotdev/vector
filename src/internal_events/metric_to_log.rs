use super::prelude::error_stage;
use metrics::counter;
use serde_json::Error;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct MetricToLogSerializeError {
    pub error: Error,
}

impl<'a> InternalEvent for MetricToLogSerializeError {
    fn emit_logs(&self) {
        error!(
            message = "Metric failed to serialize as JSON.",
            error = ?self.error,
            error_type = "serialize_failed",
            stage = error_stage::PROCESSING,
            internal_log_rate_secs = 30
        )
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error" => self.error.to_string(),
            "error_type" => "serialize_failed",
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!("processing_errors_total", 1, "error_type" => "failed_serialize");
    }
}
