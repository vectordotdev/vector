use super::prelude::error_stage;
use metrics::counter;
use vector_core::internal_event::InternalEvent;

use crate::event::metric::{MetricKind, MetricValue};

#[derive(Debug)]
pub struct StatsdInvalidMetricError<'a> {
    pub value: &'a MetricValue,
    pub kind: &'a MetricKind,
}

impl<'a> InternalEvent for StatsdInvalidMetricError<'a> {
    fn emit_logs(&self) {
        error!(
            message = "Invalid metric received; dropping event.",
            error_code = "invalid_metric",
            error_type = "encoder_failed",
            stage = error_stage::PROCESSING,
            value = ?self.value,
            kind = ?self.kind,
            internal_log_rate_secs = 10,
        )
    }

    fn emit_metrics(&self) {
        counter!(
            "processing_errors_total", 1,
            "error_code" => "invalid_metric",
            "error_type" => "encoder_failed",
            "stage" => error_stage::PROCESSING,
        );
    }
}
