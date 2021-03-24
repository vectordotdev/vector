use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct ExecEventReceived {
    pub byte_size: usize,
}

impl InternalEvent for ExecEventReceived {
    fn emit_logs(&self) {
        trace!(message = "Received one event.");
    }

    fn emit_metrics(&self) {
        counter!("processed_events_total", 1);
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct ExecFailed {
    pub error: std::io::Error,
}

impl InternalEvent for ExecFailed {
    fn emit_logs(&self) {
        error!(message = "Unable to exec.", error = ?self.error);
    }

    fn emit_metrics(&self) {
        counter!("processing_errors_total", 1);
    }
}

#[derive(Debug)]
pub struct ExecTimeout {
    pub elapsed_millis: u64,
}

impl InternalEvent for ExecTimeout {
    fn emit_logs(&self) {
        error!(
            message = "Timeout during exec.",
            elapsed_millis = self.elapsed_millis
        );
    }

    fn emit_metrics(&self) {
        counter!("processing_errors_total", 1);
    }
}
