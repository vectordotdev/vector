use super::InternalEvent;
use metrics::counter;

/// Emitted when reflector gets a desync from the watch command.
#[derive(Debug)]
pub struct DesyncReceived {}

impl InternalEvent for DesyncReceived {
    fn emit_logs(&self) {
        info!(message = "Handling desync.");
    }

    fn emit_metrics(&self) {
        counter!("k8s_reflector_desyncs_total", 1);
    }
}
