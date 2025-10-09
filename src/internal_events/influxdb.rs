use metrics::counter;
use vector_config::internal_event;
use vector_lib::internal_event::{
    ComponentEventsDropped, InternalEvent, UNINTENTIONAL, error_stage, error_type,
};

#[internal_event]
#[derive(Debug)]
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
            internal_log_rate_limit = true,
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
