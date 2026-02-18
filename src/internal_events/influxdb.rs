use metrics::counter;
use vector_lib::NamedInternalEvent;
use vector_lib::internal_event::{
    ComponentEventsDropped, InternalEvent, UNINTENTIONAL, error_stage, error_type,
};

#[derive(Debug, NamedInternalEvent)]
pub struct InfluxdbEncodingError {
    pub error_message: &'static str,
    pub count: usize,
}

impl InternalEvent for InfluxdbEncodingError {
    fn emit(self) {
        let reason = "Failed to encode event.";
        error!(
            message = reason,
            error = %self.error_message,
            error_type = error_type::ENCODER_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            "component_errors_total",
            "error_type" => error_type::ENCODER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);

        emit!(ComponentEventsDropped::<UNINTENTIONAL> {
            count: self.count,
            reason
        });
    }
}
