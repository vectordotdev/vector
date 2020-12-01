use super::InternalEvent;

#[derive(Debug)]
pub struct FilterEventProcessed;

impl InternalEvent for FilterEventProcessed {
    fn emit_metrics(&self) {
        counter!("events_processed", 1,
            "component_kind" => "transform",
            "component_type" => "filter",
        );
    }
}

#[derive(Debug)]
pub struct FilterEventDiscard;

impl InternalEvent for FilterEventProcessed {
    fn emit_metrics(&self) {
        counter!("events_discarded", 1,
            "component_kind" => "transform",
            "component_type" => "filter",
        );
    }
}

#[derive(Debug)]
pub struct FilterEventCheckError {
    pub error: dyn std::error::Error,
}

impl InternalEvent for FilterEventCheckError {
    fn emit_logs(&self) {
        error!(message = "Error in lua script; discarding event.", error = ?self.error, rate_limit_secs = 30);
    }

    fn emit_metrics(&self) {
        counter!("processing_errors_total", 1);
    }
}
