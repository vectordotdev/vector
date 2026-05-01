use serde_json::Error;
use vector_lib::internal_event::{
    ComponentEventsDropped, CounterName, InternalEvent, UNINTENTIONAL, error_stage, error_type,
};
use vector_lib::{NamedInternalEvent, counter};

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
            CounterName::ComponentErrorsTotal,
            "error_type" => error_type::ENCODER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);

        emit!(ComponentEventsDropped::<UNINTENTIONAL> { count: 1, reason })
    }
}
