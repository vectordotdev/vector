use metrics::counter;
use vector_lib::internal_event::InternalEvent;
use vector_lib::internal_event::{error_stage, error_type, ComponentEventsDropped, UNINTENTIONAL};

#[derive(Debug)]
pub struct DatadogTracesEncodingError {
    pub error_message: &'static str,
    pub error_reason: String,
    pub dropped_events: usize,
}

impl InternalEvent for DatadogTracesEncodingError {
    fn emit(self) {
        let reason = "Failed to encode Datadog traces.";
        error!(
            message = reason,
            error = %self.error_message,
            error_reason = %self.error_reason,
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

        if self.dropped_events > 0 {
            emit!(ComponentEventsDropped::<UNINTENTIONAL> {
                count: self.dropped_events,
                reason,
            });
        }
    }
}

#[derive(Debug)]
pub struct DatadogTracesAPMStatsError<E> {
    pub error: E,
}

impl<E: std::fmt::Display> InternalEvent for DatadogTracesAPMStatsError<E> {
    fn emit(self) {
        error!(
            message = "Failed sending APM stats payload.",
            error = %self.error,
            error_type = error_type::WRITER_FAILED,
            stage = error_stage::SENDING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "error_type" => error_type::WRITER_FAILED,
            "stage" => error_stage::SENDING,
        )
        .increment(1);

        // No dropped events because APM stats payloads are not considered events.
    }
}
