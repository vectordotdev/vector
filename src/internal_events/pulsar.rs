use super::prelude::error_stage;
use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub(crate) struct PulsarEncodeEventError<E> {
    pub error: E,
}

impl<E: std::fmt::Display> InternalEvent for PulsarEncodeEventError<E> {
    fn emit_logs(&self) {
        error!(
            message = "Event encode failed; dropping event.",
            error = %self.error,
            error_code = "pulsar_encoding_failed",
            error_type = "encoder_failed",
            stage = error_stage::PROCESSING,
            internal_log_rate_secs = 30,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error_code" => "pulsar_encoding_failed",
            "error_type" => "encoder_failed",
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!("encode_errors_total", 1);
    }
}
