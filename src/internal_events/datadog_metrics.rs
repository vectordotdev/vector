use vector_lib::internal_event::{
    ComponentEventsDropped, CounterName, InternalEvent, UNINTENTIONAL, error_stage, error_type,
};
use vector_lib::{NamedInternalEvent, counter};

#[derive(Debug, NamedInternalEvent)]
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
        );
        counter!(
            CounterName::ComponentErrorsTotal,
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
