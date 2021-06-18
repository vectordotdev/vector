use super::InternalEvent;
use metrics::counter;

#[derive(Debug, Copy, Clone)]
pub struct VrlConditionExecutionError;

impl InternalEvent for VrlConditionExecutionError {
    fn emit_logs(&self) {
        warn!(
            message = "VRL condition execution failed.",
            internal_log_rate_secs = 120
        )
    }

    fn emit_metrics(&self) {
        counter!("processing_errors_total", 1);
    }
}
