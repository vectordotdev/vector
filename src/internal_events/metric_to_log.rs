use metrics::counter;
use serde_json::Error;
use vector_core::internal_event::InternalEvent;

use vector_common::internal_event::{error_stage, error_type};

#[derive(Debug)]
pub struct MetricToLogSerializeError {
    pub error: Error,
}

impl InternalEvent for MetricToLogSerializeError {
    fn emit(self) {
        error!(
            message = "Metric failed to serialize as JSON.",
            error = ?self.error,
            error_type = error_type::ENCODER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::ENCODER_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!("processing_errors_total", 1, "error_type" => "failed_serialize");
    }
}
