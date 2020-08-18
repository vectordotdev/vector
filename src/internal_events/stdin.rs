use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct StdinEventReceived {
    pub byte_size: usize,
}

impl InternalEvent for StdinEventReceived {
    fn emit_logs(&self) {
        trace!(message = "Received one event.");
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
