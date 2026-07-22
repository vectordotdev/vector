use vector_lib::{
    NamedInternalEvent, counter,
    internal_event::{
        ComponentEventsDropped, CounterName, InternalEvent, UNINTENTIONAL, error_stage, error_type,
    },
};

use crate::event::metric::Metric;

#[derive(Debug, NamedInternalEvent)]
pub struct SematextMetricsInvalidMetricError<'a> {
    pub metric: &'a Metric,
}

impl InternalEvent for SematextMetricsInvalidMetricError<'_> {
    fn emit(self) {
        let reason = "Invalid metric received.";
        error!(
            message = reason,
            error_code = "invalid_metric",
            error_type =  error_type::ENCODER_FAILED,
            stage = error_stage::PROCESSING,
            value = ?self.metric.value(),
            kind = ?self.metric.kind(),
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_code" => "invalid_metric",
            "error_type" => error_type::ENCODER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);

        emit!(ComponentEventsDropped::<UNINTENTIONAL> { count: 1, reason });
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct SematextMetricsEncodeEventError<E> {
    pub error: E,
}

impl<E: std::fmt::Display> InternalEvent for SematextMetricsEncodeEventError<E> {
    fn emit(self) {
        let reason = "Failed to encode event.";
        error!(
            message = reason,
            error = %self.error,
            error_type = error_type::ENCODER_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_type" => error_type::ENCODER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);

        emit!(ComponentEventsDropped::<UNINTENTIONAL> { count: 1, reason });
    }
}
