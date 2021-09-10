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
        counter!("received_events_total", 1);
        counter!("events_in_total", 1);
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct StdinReadFailed {
    pub error: std::io::Error,
}

impl InternalEvent for StdinReadFailed {
    fn emit_logs(&self) {
        error!(message = "Unable to read from source.", error = ?self.error);
    }

    fn emit_metrics(&self) {
        counter!("stdin_reads_failed_total", 1);
    }
}
