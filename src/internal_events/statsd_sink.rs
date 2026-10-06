use vector_lib::internal_event::{
    ComponentEventsDropped, CounterName, InternalEvent, UNINTENTIONAL, error_stage, error_type,
};
use vector_lib::{NamedInternalEvent, counter};

use crate::event::metric::{MetricKind, MetricValue};

#[derive(Debug, NamedInternalEvent)]
pub struct StatsdInvalidMetricError<'a> {
    pub value: &'a MetricValue,
    pub kind: MetricKind,
}

impl InternalEvent for StatsdInvalidMetricError<'_> {
    fn emit(self) {
        let reason = "Invalid metric type received.";
        error!(
            message = reason,
            error_code = "invalid_metric",
            error_type = error_type::ENCODER_FAILED,
            stage = error_stage::PROCESSING,
            value = ?self.value,
            kind = ?self.kind,
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_code" => "invalid_metric",
            "error_type" => error_type::ENCODER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);

        emit!(ComponentEventsDropped::<UNINTENTIONAL> { reason, count: 1 });
    }
}
