use metrics::counter;
use serde_json::Error;
use vector_lib::NamedInternalEvent;
use vector_lib::internal_event::{
    ComponentEventsDropped, InternalEvent, UNINTENTIONAL, error_stage, error_type,
};

#[derive(Debug, NamedInternalEvent)]
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
            stage = error_stage::PROCESSING
        );
        counter!(
            "component_errors_total",
            "error_type" => error_type::ENCODER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);

        emit!(ComponentEventsDropped::<UNINTENTIONAL> { count: 1, reason })
    }
}
