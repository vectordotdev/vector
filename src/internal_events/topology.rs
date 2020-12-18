use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct EventProcessed;

impl InternalEvent for EventProcessed {
    fn emit_metrics(&self) {
        counter!("processed_events_total", 1);
    }
}
