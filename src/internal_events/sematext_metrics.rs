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
            internal_log_rate_secs = 30,
        )
    }

    fn emit_metrics(&self) {
        counter!(
            "processing_errors_total", 1,
            "error_type" => "invalid_metric",
        );
    }
}

#[derive(Debug)]
pub struct SematextMetricsEncodeEventFailed {
    pub error: &'static str,
}

impl InternalEvent for SematextMetricsEncodeEventFailed {
    fn emit_logs(&self) {
        warn!(
             message = "Failed to encode event; dropping event.",
             error = %self.error,
             internal_log_rate_secs = 30
        );
    }

    fn emit_metrics(&self) {
        counter!("encode_errors_total", 1);
    }
}
