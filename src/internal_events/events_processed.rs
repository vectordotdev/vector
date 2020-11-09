use crate::internal_events::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct EventProcessed;

impl InternalEvent for EventProcessed {
    fn emit_metrics(&self) {
        counter!("events_processed_total", 1);
    }
}
