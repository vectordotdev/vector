use metrics::counter;
use serde_json::Error;
use vector_lib::internal_event::InternalEvent;
use vector_lib::internal_event::{error_stage, error_type, ComponentEventsDropped, UNINTENTIONAL};

#[derive(Debug)]
pub struct MetricToLogSerializeError {
    pub error: Error,
}

impl InternalEvent for MetricToLogSerializeError {
    fn emit(self) {
        let reason = "Metric failed to serialize as JSON.";
        error!(
            message = reason,
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

        emit!(ComponentEventsDropped::<UNINTENTIONAL> { count: 1, reason })
    }
}
