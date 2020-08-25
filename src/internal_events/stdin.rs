use super::InternalEvent;
use metrics::counter;

define_events_processed_bytes!(StdinEventReceived, "source", "stdin");

#[derive(Debug)]
pub struct StdinReadFailed {
    pub error: std::io::Error,
}

impl InternalEvent for StdinReadFailed {
    fn emit_logs(&self) {
        error!(message = "unable to read from source.", error = %self.error);
    }

    fn emit_metrics(&self) {
        counter!(
            "stdin_reads_failed", 1,
            "component_kind" => "source",
            "component_type" => "stdin",
        );
    }
}
