use metrics::counter;
use vector_lib::internal_event::InternalEvent;

use crate::event::metric::{MetricKind, MetricValue};
use vector_lib::internal_event::{ComponentEventsDropped, UNINTENTIONAL, error_stage, error_type};

#[derive(Debug)]
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
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "error_code" => "invalid_metric",
            "error_type" => error_type::ENCODER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);

        emit!(ComponentEventsDropped::<UNINTENTIONAL> { reason, count: 1 });
    }
}
