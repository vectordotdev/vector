use metrics::counter;
use vector_core::internal_event::InternalEvent;

use crate::event::metric::{MetricKind, MetricValue};
use crate::{
    emit,
    internal_events::{ComponentEventsDropped, UNINTENTIONAL},
};
use vector_common::internal_event::{error_stage, error_type};

#[derive(Debug)]
pub struct StatsdInvalidMetricError<'a> {
    pub value: &'a MetricValue,
    pub kind: &'a MetricKind,
}

impl<'a> InternalEvent for StatsdInvalidMetricError<'a> {
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
            "component_errors_total", 1,
            "error_code" => "invalid_metric",
            "error_type" => error_type::ENCODER_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!("processing_errors_total", 1);

        emit!(ComponentEventsDropped::<UNINTENTIONAL> { reason, count: 1 });
    }
}
