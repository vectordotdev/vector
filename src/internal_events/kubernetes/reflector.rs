use metrics::counter;
use vector_core::internal_event::InternalEvent;

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

pub struct InvocationHttpErrorReceived<E> {
    /// The underlying error.
    pub error: E,
}

impl<E: std::fmt::Debug> InternalEvent for InvocationHttpErrorReceived<E> {
    fn emit_logs(&self) {
        warn!(message = "Http Error in invocation! Your k8s metadata may be stale. Continuing Loop.", error = ?self.error);
    }

    fn emit_metrics(&self) {
        counter!("k8s_watcher_http_error_total", 1);
    }
}
