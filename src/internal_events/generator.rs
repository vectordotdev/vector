use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct GeneratorEventProcessed;

impl InternalEvent for GeneratorEventProcessed {
    fn emit_logs(&self) {
        trace!(message = "Received one event.");
    }

    fn emit_metrics(&self) {
        counter!("events_processed", 1);
    }
}
