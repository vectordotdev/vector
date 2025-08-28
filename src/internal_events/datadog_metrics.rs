use metrics::counter;
use vector_lib::internal_event::InternalEvent;
use vector_lib::internal_event::{ComponentEventsDropped, UNINTENTIONAL, error_stage, error_type};

#[derive(Debug)]
pub struct DatadogMetricsEncodingError<'a> {
    pub reason: &'a str,
    pub error_code: &'static str,
    pub dropped_events: usize,
}

impl InternalEvent for DatadogMetricsEncodingError<'_> {
    fn emit(self) {
        error!(
            message = self.reason,
            error_code = self.error_code,
            error_type = error_type::ENCODER_FAILED,
            intentional = "false",
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "error_code" => self.error_code,
            "error_type" => error_type::ENCODER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);

        if self.dropped_events > 0 {
            emit!(ComponentEventsDropped::<UNINTENTIONAL> {
                count: self.dropped_events,
                reason: self.reason,
            });
        }
    }
}
