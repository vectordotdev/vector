use super::InternalEvent;
use crate::event::metric::{MetricKind, MetricValue};
use metrics::counter;

#[derive(Debug)]
pub struct SematextMetricsInvalidMetricReceived {
    pub value: MetricValue,
    pub kind: MetricKind,
}

impl InternalEvent for SematextMetricsInvalidMetricReceived {
    fn emit_logs(&self) {
        warn!(
            message = "Invalid metric received; dropping event.",
            value = ?self.value,
            kind = ?self.kind,
            rate_limit_secs = 30,
        )
    }

    fn emit_metrics(&self) {
        counter!(
            "processing_errors", 1,
            "component_kind" => "sink",
            "component_type" => "sematext_metrics",
            "error_type" => "invalid_metric",
        );
    }
}
