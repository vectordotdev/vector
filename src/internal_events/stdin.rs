use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct StdinEventReceived {
    pub byte_size: usize,
}

impl InternalEvent for StdinEventReceived {
    fn emit_logs(&self) {
        trace!(message = "received one event.", rate_limit_secs = 10);
    }

    fn emit_metrics(&self) {
        counter!(
            "events_processed", 1,
            "component_kind" => "source",
            "component_type" => "stdin",
        );
        counter!(
            "bytes_processed", self.byte_size as u64,
            "component_kind" => "source",
            "component_type" => "stdin",
        );
    }
}
