use super::InternalEvent;
use metrics::counter;

/// Emitted when reflector gets a desync from the watch command.
#[derive(Debug)]
pub struct InvocationDesyncReceived<E> {
    /// The underlying error.
    pub error: E,
}

impl<E: std::fmt::Debug> InternalEvent for InvocationDesyncReceived<E> {
    fn emit_logs(&self) {
        warn!(message = "Handling desync.", error = ?self.error);
    }

    fn emit_metrics(&self) {
        counter!("k8s_reflector_desyncs_total", 1);
    }
}

/// Emitted when reflector gets a desync from the watch command.
#[derive(Debug)]
pub struct StreamDesyncReceived<E> {
    /// The underlying error.
    pub error: E,
}

impl<E: std::fmt::Debug> InternalEvent for StreamDesyncReceived<E> {
    fn emit_logs(&self) {
        warn!(message = "Handling desync.", error = ?self.error);
    }

    fn emit_metrics(&self) {
        counter!("k8s_reflector_desyncs_total", 1);
    }
}
