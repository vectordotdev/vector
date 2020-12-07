use super::InternalEvent;
use metrics::counter;
use serde_json::Error;

#[derive(Debug)]
pub struct FilterEventProcessed;

impl InternalEvent for FilterEventProcessed {
    fn emit_metrics(&self) {
        counter!("events_processed", 1);
    }
}

#[derive(Debug)]
pub struct FilterEventDiscarded;

impl InternalEvent for FilterEventDiscarded {
    fn emit_metrics(&self) {
        counter!("events_discarded", 1);
    }
}

#[derive(Debug)]
pub struct FilterEventError {
    pub error: Error,
}

impl InternalEvent for FilterEventError {
    fn emit_logs(&self) {
        error!(message = "Error in filter; discarding event.", error = ?self.error, rate_limit_secs = 30);
    }

    fn emit_metrics(&self) {
        counter!("processing_errors_total", 1);
    }
}
