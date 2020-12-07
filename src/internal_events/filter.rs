use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct FilterEventProcessed;

impl InternalEvent for FilterEventProcessed {
    fn emit_metrics(&self) {
        counter!("events_processed", 1);
    }
}

#[derive(Debug)]
pub struct FilterEventDiscarded;

impl InternalEvent for FilterEventDiscarded {
    fn emit_metrics(&self) {
        counter!("events_discarded", 1);
    }
}
