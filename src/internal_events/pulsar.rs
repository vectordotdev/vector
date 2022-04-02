use super::prelude::{error_stage, error_type};
use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub(crate) struct PulsarEncodeEventError<E> {
    pub error: E,
}

impl<E: std::fmt::Display> InternalEvent for PulsarEncodeEventError<E> {
    fn emit(self) {
        error!(
            message = "Event encode failed; dropping event.",
            error = %self.error,
            error_code = "pulsar_encoding",
            error_type = error_type::ENCODER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_secs = 30,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "pulsar_encoding",
            "error_type" => error_type::ENCODER_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!("encode_errors_total", 1);
    }
}
