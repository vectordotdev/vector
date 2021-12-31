use metrics::counter;
use vector_core::internal_event::InternalEvent;

use crate::event::metric::Metric;

#[derive(Debug)]
pub struct SematextMetricsInvalidMetricReceived<'a> {
    pub metric: &'a Metric,
}

impl<'a> InternalEvent for SematextMetricsInvalidMetricReceived<'a> {
    fn emit_logs(&self) {
        warn!(
            message = "Invalid metric received; dropping event.",
            value = ?self.metric.value(),
            kind = ?self.metric.kind(),
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
