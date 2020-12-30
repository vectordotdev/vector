use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct RemapMappingError {
    /// If set to true, the remap transform has dropped the event after a failed
    /// mapping. This internal event will reflect that in its messaging.
    pub event_dropped: bool,
    pub error: String,
}

impl InternalEvent for RemapMappingError {
    fn emit_logs(&self) {
        let message = if self.event_dropped {
            "Mapping failed with event; discarding event."
        } else {
            "Mapping failed with event."
        };

        warn!(
            message,
            error = ?self.error,
            internal_log_rate_secs = 30
        )
    }

    fn emit_metrics(&self) {
        counter!("processing_errors_total", 1,
                 "error_type" => "failed_mapping");
    }
}

#[derive(Debug, Copy, Clone)]
pub struct RemapConditionExecutionError;

impl InternalEvent for RemapConditionExecutionError {
    fn emit_logs(&self) {
        warn!(
            message = "Remap condition execution failed.",
            internal_log_rate_secs = 120
        )
    }

    fn emit_metrics(&self) {
        counter!("processing_errors_total", 1);
    }
}
