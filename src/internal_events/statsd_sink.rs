use metrics::counter;
use vector_core::internal_event::InternalEvent;

use super::prelude::{error_stage, error_type};
use crate::event::metric::{MetricKind, MetricValue};

#[derive(Debug)]
pub struct StatsdInvalidMetricError<'a> {
    pub value: &'a MetricValue,
    pub kind: &'a MetricKind,
}

impl<'a> InternalEvent for StatsdInvalidMetricError<'a> {
    fn emit(self) {
        error!(
            message = "Invalid metric received; dropping event.",
            error_code = "invalid_metric",
            error_type = error_type::ENCODER_FAILED,
            stage = error_stage::PROCESSING,
            value = ?self.value,
            kind = ?self.kind,
            internal_log_rate_secs = 10,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "invalid_metric",
            "error_type" => error_type::ENCODER_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!("processing_errors_total", 1);
    }
}
