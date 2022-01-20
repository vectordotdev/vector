use crate::source_sender::ClosedError;
use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct DatadogAgentStreamError {
    pub error: ClosedError,
    pub count: usize,
}

impl InternalEvent for DatadogAgentStreamError {
    fn emit_logs(&self) {
        error!(
            message = "Failed to forward events, downstream is closed.",
            error = %self.error,
            error_type = "stream",
            stage = "sending",
            count = %self.count,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", self.count as u64,
            "error" => self.error.to_string(),
            "error_type" => "stream",
            "stage" => "sending",
        );
        counter!(
            "component_discarded_events_total", self.count as u64,
            "error" => self.error.to_string(),
            "error_type" => "stream",
            "stage" => "sending",
        );
    }
}
