use super::InternalEvent;
use crate::event::metric::{MetricKind, MetricValue};
use metrics::counter;

#[derive(Debug)]
pub struct SematextMetricsInvalidMetric {
    pub value: MetricValue,
    pub kind: MetricKind,
}

impl InternalEvent for SematextMetricsInvalidMetric {
    fn emit_logs(&self) {
        warn!(
            message = "invalid metric sent; dropping event.",
            value = ?self.value,
            kind = ?self.kind,
            rate_limit_secs = 30,
        )
    }

    fn emit_metrics(&self) {
        counter!(
            "invalid_metrics", 1,
            "component_kind" => "sink",
            "component_type" => "sematext_metrics",
        );
    }
}
